pub mod auth;
mod handlers;
mod router;
mod ui;

use std::{
    fs, io,
    net::{TcpListener, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use serde::Serialize;
use tiny_http::Server;

use crate::config::RuntimePolicy;

use self::{auth::AdminToken, router::AdminState};

pub struct BoundServer {
    pub server: Server,
    pub port: u16,
}

pub fn candidate_ports(base: u16) -> impl Iterator<Item = u16> {
    base..=base.saturating_add(9)
}

pub fn start(
    plugin_root: PathBuf,
    database_path: PathBuf,
    policy: RuntimePolicy,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        if let Err(error) = run(&plugin_root, database_path, policy) {
            eprintln!("[Luo Realm] admin server stopped: {error}");
        }
    })
}

fn run(plugin_root: &Path, database_path: PathBuf, policy: RuntimePolicy) -> io::Result<()> {
    let config = policy.snapshot();
    let data_directory = plugin_root.join(crate::identity::DATA_DIRECTORY);
    let token_path = data_directory.join("admin.token");
    let token = AdminToken::load_or_create(&token_path).map_err(io::Error::other)?;
    let bound = bind_server(&config.admin.bind, config.admin.port)?;
    write_runtime_file(&data_directory, &config.admin.bind, bound.port)?;
    let state = Arc::new(AdminState {
        plugin_root: plugin_root.to_path_buf(),
        database_path,
        token_path,
        token,
        policy,
        port: bound.port,
    });
    eprintln!(
        "[Luo Realm] admin console: http://{}:{}",
        config.admin.bind, bound.port
    );

    bound.server.incoming_requests().for_each(|mut request| {
        let response = router::route(&mut request, &state);
        if let Err(error) = request.respond(response) {
            eprintln!("[Luo Realm] admin response failed: {error}");
        }
    });
    Ok(())
}

#[derive(Serialize)]
struct RuntimeInfo<'a> {
    process_id: u32,
    bind: &'a str,
    port: u16,
    started_at: i64,
}

fn write_runtime_file(data_directory: &Path, bind: &str, port: u16) -> io::Result<()> {
    fs::create_dir_all(data_directory)?;
    let path = data_directory.join("admin.runtime.json");
    let temporary = data_directory.join("admin.runtime.json.new");
    let backup = data_directory.join("admin.runtime.json.bak");
    let info = RuntimeInfo {
        process_id: std::process::id(),
        bind,
        port,
        started_at: crate::database::unix_timestamp(),
    };
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    fs::write(&temporary, serde_json::to_vec_pretty(&info)?)?;
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    if path.exists() {
        fs::rename(&path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::rename(&backup, &path);
        return Err(error);
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

pub fn bind_server(bind: &str, base_port: u16) -> io::Result<BoundServer> {
    for port in candidate_ports(base_port) {
        let address = (bind, port).to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::AddrNotAvailable, "invalid bind address")
        })?;
        match TcpListener::bind(address) {
            Ok(listener) => {
                let server = Server::from_listener(listener, None).map_err(io::Error::other)?;
                return Ok(BoundServer { server, port });
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        format!("ports {base_port} through {} are occupied", base_port + 9),
    ))
}
