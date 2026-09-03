//! 玩家网页（只读档案页）。
//!
//! 群聊命令 `主页` 签发一次性票据并返回页面链接；页面用票据换取短期无状
//! 态会话，之后只能调用 `profile:read` 范围内的只读接口。写入型操作仍由
//! 群聊命令完成（设计方案书 20.3、27.2）。本模块是 P6“玩家网页与节点地
//! 图”的只读前置切片，不提供任何修改权威状态的入口。

mod handlers;
pub mod session;
pub mod ticket;
mod views;

pub(crate) use handlers::route;
