'use strict';

const byId = id => document.getElementById(id);
const state = {
    token: sessionStorage.getItem('lr_admin_token') || '',
    config: null,
    player: null,
    assetPage: 1,
    assetUrls: [],
};

function element(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = String(text);
    return node;
}

function makeButton(text, handler, className = 'button') {
    const button = element('button', className, text);
    button.type = 'button';
    button.addEventListener('click', handler);
    return button;
}

function tableCell(row, text) {
    const cell = element('td', '', text ?? '');
    row.appendChild(cell);
    return cell;
}

function inputField(labelText, id, value, type = 'text') {
    const label = element('label');
    const caption = element('span', '', labelText);
    const input = document.createElement('input');
    input.id = id;
    input.type = type;
    input.value = value ?? '';
    label.append(caption, input);
    return label;
}

function notify(message, bad = false) {
    const node = byId('notice');
    node.textContent = message;
    node.classList.toggle('bad', bad);
    node.classList.remove('hidden');
    window.clearTimeout(notify.timer);
    notify.timer = window.setTimeout(() => node.classList.add('hidden'), 4200);
}

async function api(path, options = {}) {
    const headers = new Headers(options.headers || {});
    headers.set('Authorization', `Bearer ${state.token}`);
    if (typeof options.body === 'string') headers.set('Content-Type', 'application/json');
    const response = await fetch(path, { ...options, headers });
    const payload = await response.json().catch(() => ({ ok: false, error: { message: '响应格式错误' } }));
    if (!response.ok || !payload.ok) throw new Error(payload.error?.message || '请求失败');
    return payload.data;
}

async function rawRequest(path, options = {}) {
    const headers = new Headers(options.headers || {});
    headers.set('Authorization', `Bearer ${state.token}`);
    const response = await fetch(path, { ...options, headers });
    if (!response.ok) {
        const payload = await response.json().catch(() => null);
        throw new Error(payload?.error?.message || `请求失败 (${response.status})`);
    }
    return response;
}

async function download(path, fallbackName) {
    const response = await rawRequest(path);
    const blob = await response.blob();
    const disposition = response.headers.get('Content-Disposition') || '';
    const matched = disposition.match(/filename="([^"]+)"/);
    const anchor = document.createElement('a');
    anchor.href = URL.createObjectURL(blob);
    anchor.download = matched?.[1] || fallbackName;
    anchor.click();
    URL.revokeObjectURL(anchor.href);
}

function requestAction({ title, message = '', confirmation = '', danger = false }) {
    const dialog = byId('actionDialog');
    dialog.returnValue = '';
    byId('actionTitle').textContent = title;
    byId('actionMessage').textContent = message;
    byId('actionReason').value = '';
    byId('actionConfirmation').value = '';
    byId('actionConfirmation').placeholder = confirmation;
    byId('confirmationField').classList.toggle('hidden', !confirmation);
    byId('actionSubmit').className = danger ? 'button danger' : 'button primary';
    dialog.showModal();

    return new Promise(resolve => {
        dialog.addEventListener('close', () => {
            resolve(dialog.returnValue === 'confirm' ? {
                reason: byId('actionReason').value.trim(),
                confirm: confirmation || undefined,
            } : null);
        }, { once: true });
    });
}

byId('actionForm').addEventListener('submit', event => {
    if (event.submitter?.value !== 'confirm') return;
    const reason = byId('actionReason');
    const confirmation = byId('actionConfirmation');
    const expected = confirmation.placeholder;
    confirmation.setCustomValidity(expected && confirmation.value !== expected ? `请输入：${expected}` : '');
    if (!reason.checkValidity() || !confirmation.checkValidity()) {
        event.preventDefault();
        byId('actionForm').reportValidity();
    }
});

async function loadDashboard() {
    const data = await api('/api/overview');
    const metrics = [
        ['玩家', data.players],
        ['完整角色', data.active_players],
        ['待选体系', data.pending_players],
        ['启用群', data.enabled_groups],
        ['战斗记录', data.combats],
        ['钱包流水', data.wallet_transactions],
    ];
    byId('stats').replaceChildren(...metrics.map(([label, value]) => {
        const card = element('div', 'stat');
        card.append(element('strong', '', value), element('span', '', label));
        return card;
    }));
}

async function loadGroups() {
    const search = encodeURIComponent(byId('groupSearch').value.trim());
    const data = await api(`/api/groups?search=${search}`);
    const rows = data.items.map(group => {
        const row = document.createElement('tr');
        tableCell(row, group.group_id);
        ['enabled', 'general', 'event', 'combat'].forEach(key => {
            const input = document.createElement('input');
            input.type = 'checkbox';
            input.className = 'switch';
            input.checked = group[key];
            input.dataset.key = key;
            tableCell(row, '').appendChild(input);
        });
        const reportMode = document.createElement('select');
        reportMode.dataset.key = 'battle_report_mode';
        [['inherit', '跟随全局'], ['enabled', '开启'], ['disabled', '关闭']].forEach(([value, label]) => {
            const option = document.createElement('option');
            option.value = value;
            option.textContent = label;
            reportMode.appendChild(option);
        });
        reportMode.value = group.battle_report_mode;
        tableCell(row, '').appendChild(reportMode);
        tableCell(row, new Date(group.updated_at * 1000).toLocaleString());
        tableCell(row, '').appendChild(makeButton('保存', () => saveGroup(group, row)));
        return row;
    });
    byId('groupRows').replaceChildren(...rows);
}

async function saveGroup(group, row) {
    const values = Object.fromEntries([...row.querySelectorAll('input')].map(input => [input.dataset.key, input.checked]));
    values.battle_report_mode = row.querySelector('select[data-key="battle_report_mode"]').value;
    const confirmation = values.enabled === false && group.enabled ? `group:${group.group_id}:disable` : '';
    const action = await requestAction({ title: '保存群聊策略', message: `群 ${group.group_id}`, confirmation, danger: Boolean(confirmation) });
    if (!action) return;
    try {
        if (values.enabled !== group.enabled) {
            await api(`/api/groups/${group.group_id}`, { method: 'PUT', body: JSON.stringify({ enabled: values.enabled, ...action }) });
        }
        await api(`/api/groups/${group.group_id}/features`, { method: 'PUT', body: JSON.stringify({ general: values.general, event: values.event, combat: values.combat, battle_report_mode: values.battle_report_mode, reason: action.reason }) });
        notify('群聊策略已保存');
        await loadGroups();
    } catch (error) { notify(error.message, true); }
}

async function addGroup() {
    const groupId = byId('groupSearch').value.trim();
    if (!/^\d+$/.test(groupId)) return notify('请先在搜索框输入要启用的群号', true);
    const action = await requestAction({ title: '启用群聊', message: `群 ${groupId}` });
    if (!action) return;
    try {
        await api('/api/groups', { method: 'POST', body: JSON.stringify({ group_id: Number(groupId), enabled: true, reason: action.reason }) });
        notify('群聊已启用');
        await loadGroups();
    } catch (error) { notify(error.message, true); }
}

async function loadPlayers() {
    const search = encodeURIComponent(byId('playerSearch').value.trim());
    const data = await api(`/api/players?search=${search}`);
    byId('playerTotal').textContent = `${data.total} 名玩家`;
    const rows = data.items.map(player => {
        const row = document.createElement('tr');
        const identity = tableCell(row, '');
        identity.append(element('span', 'row-title', player.display_name), element('span', 'row-subtitle', player.player_id));
        tableCell(row, player.registration_state === 'pending_system' ? '待选体系' : `${player.system_id} · ${Number(player.realm_index) + 1}`);
        tableCell(row, '').appendChild(makeButton('管理', () => loadPlayer(player.player_id)));
        return row;
    });
    byId('playerRows').replaceChildren(...rows);
}

async function loadPlayer(playerId) {
    try {
        state.player = await api(`/api/players/${playerId}`);
        renderPlayer(state.player);
    } catch (error) { notify(error.message, true); }
}

function renderPlayer(data) {
    const root = byId('playerDetail');
    root.classList.remove('empty-state');
    root.replaceChildren();
    const heading = element('div', 'page-heading');
    const name = element('div');
    name.append(element('h2', '', data.player.display_name), element('span', 'muted', `QQ ${data.player.player_id}`));
    heading.append(name, element('span', 'counter', data.player.registration_state === 'active' ? '完整角色' : '待选体系'));
    root.appendChild(heading);

    const tabs = element('div', 'tabs');
    const bodies = element('div');
    [['资料', 'profile'], ['钱包', 'wallet'], ['修行', 'cultivation'], ['物品', 'items'], ['统计', 'statistics']].forEach(([label, key], index) => {
        const tab = makeButton(label, () => selectPlayerTab(key), '');
        tab.dataset.tab = key;
        tab.classList.toggle('active', index === 0);
        tabs.appendChild(tab);
        const body = element('div', `tab-body${index === 0 ? ' active' : ''}`);
        body.id = `player-${key}`;
        bodies.appendChild(body);
    });
    root.append(tabs, bodies);
    renderProfile(data);
    renderWallet(data);
    renderCultivation(data);
    renderItems(data);
    renderStatistics(data);
}

function selectPlayerTab(key) {
    document.querySelectorAll('#playerDetail [data-tab]').forEach(tab => tab.classList.toggle('active', tab.dataset.tab === key));
    document.querySelectorAll('#playerDetail .tab-body').forEach(body => body.classList.toggle('active', body.id === `player-${key}`));
}

function renderProfile(data) {
    const root = byId('player-profile');
    const statusLabel = element('label');
    const status = document.createElement('select');
    status.id = 'editStatus';
    [['active', '正常'], ['disabled', '停用'], ['deleted', '软删除']].forEach(([value, label]) => {
        const option = element('option', '', label);
        option.value = value;
        option.selected = value === data.player.status;
        status.appendChild(option);
    });
    statusLabel.append(element('span', '', '状态'), status);
    root.append(inputField('显示名称', 'editName', data.player.display_name), statusLabel);
    root.appendChild(makeButton('保存资料', () => saveProfile(data.player.player_id), 'button primary'));
    const danger = element('div', 'band danger-zone');
    danger.append(element('h2', '', '永久删除玩家'), makeButton('删除玩家及关联数据', () => deletePlayer(data.player.player_id), 'button danger'));
    root.appendChild(danger);
}

async function saveProfile(playerId) {
    const status = byId('editStatus').value;
    const confirmation = status === 'active' ? '' : `player:${playerId}:${status}`;
    const action = await requestAction({ title: '保存玩家资料', confirmation, danger: Boolean(confirmation) });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}/profile`, { method: 'PUT', body: JSON.stringify({ display_name: byId('editName').value, status, ...action }) });
        notify('玩家资料已保存');
        await loadPlayer(playerId);
    } catch (error) { notify(error.message, true); }
}

async function deletePlayer(playerId) {
    const confirmation = `player:${playerId}:delete`;
    const action = await requestAction({ title: '永久删除玩家', message: '玩家、钱包流水、战斗记录和全部角色数据将被删除。', confirmation, danger: true });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}`, { method: 'DELETE', body: JSON.stringify(action) });
        notify('玩家已永久删除');
        state.player = null;
        byId('playerDetail').className = 'detail-pane empty-state';
        byId('playerDetail').textContent = '选择一名玩家';
        await loadPlayers();
    } catch (error) { notify(error.message, true); }
}

function renderWallet(data) {
    const root = byId('player-wallet');
    root.append(element('p', '', `金币 ${data.player.coins} · 刻印 ${data.player.marks}`));
    const currencyLabel = element('label');
    const currency = document.createElement('select');
    currency.id = 'walletCurrency';
    [['coins', '金币'], ['marks', '刻印']].forEach(([value, label]) => {
        const option = element('option', '', label);
        option.value = value;
        currency.appendChild(option);
    });
    currencyLabel.append(element('span', '', '货币'), currency);
    root.append(currencyLabel, inputField('变化量', 'walletDelta', 0, 'number'));
    const action = makeButton('调整钱包', () => saveWallet(data.player.player_id), 'button primary');
    action.disabled = data.player.registration_state !== 'active';
    root.appendChild(action);
}

async function saveWallet(playerId) {
    const delta = Number(byId('walletDelta').value);
    const currency = byId('walletCurrency').value;
    if (!Number.isSafeInteger(delta) || delta === 0) return notify('请输入非零整数', true);
    const confirmation = delta < 0 || Math.abs(delta) >= 10000 ? `wallet:${playerId}:${currency}:${delta}` : '';
    const action = await requestAction({ title: '调整玩家钱包', confirmation, danger: delta < 0 });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}/wallet`, { method: 'POST', body: JSON.stringify({ currency, delta, ...action }) });
        notify('钱包已调整');
        await loadPlayer(playerId);
    } catch (error) { notify(error.message, true); }
}

function renderCultivation(data) {
    const root = byId('player-cultivation');
    root.append(inputField('体系标识', 'cultSystem', data.player.system_id || ''), inputField('境界索引', 'cultRealm', data.player.realm_index ?? 0, 'number'), inputField('修行进度', 'cultProgress', data.player.progress ?? 0, 'number'));
    root.appendChild(makeButton('保存修行状态', () => saveCultivation(data.player.player_id), 'button primary'));
}

async function saveCultivation(playerId) {
    const system_id = byId('cultSystem').value.trim();
    const realm_index = Number(byId('cultRealm').value);
    const progress = Number(byId('cultProgress').value);
    const confirmation = `cultivation:${playerId}:${system_id}:${realm_index}:${progress}`;
    const action = await requestAction({ title: '修改修行状态', confirmation });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}/cultivation`, { method: 'PUT', body: JSON.stringify({ system_id, realm_index, progress, ...action }) });
        notify('修行状态已保存');
        await loadPlayer(playerId);
    } catch (error) { notify(error.message, true); }
}

function renderItems(data) {
    const root = byId('player-items');
    const table = document.createElement('table');
    table.style.minWidth = '0';
    data.items.forEach(item => {
        const row = document.createElement('tr');
        tableCell(row, `${item.definition_id} × ${item.quantity}`);
        tableCell(row, item.quality);
        tableCell(row, '').appendChild(makeButton('删除', () => deleteItem(data.player.player_id, item.item_instance_id), 'button danger'));
        table.appendChild(row);
    });
    root.append(table, inputField('物品定义 ID', 'itemDefinition', ''), inputField('数量', 'itemQuantity', 1, 'number'), inputField('品质', 'itemQuality', 'common'));
    root.appendChild(makeButton('发放物品', () => grantItem(data.player.player_id), 'button primary'));
}

async function grantItem(playerId) {
    const action = await requestAction({ title: '发放物品' });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}/items`, { method: 'POST', body: JSON.stringify({ definition_id: byId('itemDefinition').value, quantity: Number(byId('itemQuantity').value), quality: byId('itemQuality').value, reason: action.reason }) });
        notify('物品已发放');
        await loadPlayer(playerId);
    } catch (error) { notify(error.message, true); }
}

async function deleteItem(playerId, itemId) {
    const confirmation = `item:${playerId}:${itemId}:remove`;
    const action = await requestAction({ title: '删除物品', confirmation, danger: true });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}/items/${itemId}`, { method: 'DELETE', body: JSON.stringify(action) });
        notify('物品已删除');
        await loadPlayer(playerId);
    } catch (error) { notify(error.message, true); }
}

function renderStatistics(data) {
    const root = byId('player-statistics');
    data.statistics.forEach(stat => root.append(element('p', '', `${stat.metric_code}: ${stat.metric_value}`)));
    root.append(inputField('统计标识', 'statMetric', 'wins'), inputField('数值', 'statValue', 0, 'number'));
    root.appendChild(makeButton('保存统计', () => saveStatistic(data.player.player_id), 'button primary'));
}

async function saveStatistic(playerId) {
    const metric = byId('statMetric').value;
    const value = Number(byId('statValue').value);
    const confirmation = `statistic:${playerId}:${metric}:${value}`;
    const action = await requestAction({ title: '保存玩家统计', confirmation });
    if (!action) return;
    try {
        await api(`/api/players/${playerId}/statistics`, { method: 'PUT', body: JSON.stringify({ metric, value, ...action }) });
        notify('统计已保存');
        await loadPlayer(playerId);
    } catch (error) { notify(error.message, true); }
}

function clearAssetUrls() {
    state.assetUrls.forEach(URL.revokeObjectURL);
    state.assetUrls = [];
}

async function loadAssets(page = 1) {
    clearAssetUrls();
    state.assetPage = page;
    const category = encodeURIComponent(byId('assetCategory').value);
    const search = encodeURIComponent(byId('assetSearch').value.trim());
    const data = await api(`/api/assets?category=${category}&search=${search}&page=${page}&limit=60`);
    byId('assetCount').textContent = `${data.total} 个文件`;
    if (byId('assetCategory').options.length === 1) {
        data.categories.forEach(categoryItem => {
            const option = element('option', '', categoryItem.label);
            option.value = categoryItem.id;
            byId('assetCategory').appendChild(option);
        });
    }
    const cards = data.items.map(asset => {
        const card = element('article', 'asset-card');
        const preview = element('div', 'asset-preview');
        if (asset.previewable) loadAssetPreview(asset.path, preview);
        else preview.appendChild(element('span', 'asset-fallback', 'FONT'));
        const meta = element('div', 'asset-meta');
        meta.append(element('div', 'asset-name', asset.name), element('div', 'asset-path', asset.path));
        const actions = element('div', 'asset-actions');
        actions.append(element('span', 'counter', formatBytes(asset.byte_size)), makeButton('删除', () => deleteAsset(asset.path), 'button danger'));
        meta.appendChild(actions);
        card.append(preview, meta);
        return card;
    });
    byId('assetGrid').replaceChildren(...cards);
    renderAssetPagination(data);
}

async function loadAssetPreview(path, root) {
    try {
        const response = await rawRequest(`/api/assets/file?path=${encodeURIComponent(path)}`);
        const url = URL.createObjectURL(await response.blob());
        state.assetUrls.push(url);
        const image = document.createElement('img');
        image.src = url;
        image.alt = path;
        image.loading = 'lazy';
        root.appendChild(image);
    } catch { root.appendChild(element('span', 'asset-fallback', '无法预览')); }
}

function renderAssetPagination(data) {
    const pages = Math.ceil(data.total / data.limit);
    const controls = [];
    if (data.page > 1) controls.push(makeButton('上一页', () => loadAssets(data.page - 1)));
    controls.push(element('span', 'counter', `${data.page} / ${Math.max(1, pages)}`));
    if (data.page < pages) controls.push(makeButton('下一页', () => loadAssets(data.page + 1)));
    byId('assetPagination').replaceChildren(...controls);
}

function formatBytes(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}

async function uploadAsset(file) {
    const category = byId('assetCategory').value || 'portraits';
    if (category === 'fonts' && !file.name.toLowerCase().endsWith('.ttf')) {
        return notify('字体分类只能上传 TTF；请先选择一个图片分类', true);
    }
    const target = file.name.toLowerCase().endsWith('.ttf') ? 'fonts/font.ttf' : `realm/${category}/${file.name}`;
    const confirmation = `asset:${target}:write`;
    const action = await requestAction({ title: '上传素材', message: target, confirmation });
    if (!action) return;
    try {
        const query = new URLSearchParams({ path: target, reason: action.reason, confirm: confirmation });
        await api(`/api/assets/file?${query}`, { method: 'POST', body: await file.arrayBuffer(), headers: { 'Content-Type': 'application/octet-stream' } });
        notify('素材已上传');
        await loadAssets(state.assetPage);
    } catch (error) { notify(error.message, true); }
}

async function deleteAsset(path) {
    const confirmation = `asset:${path}:delete`;
    const action = await requestAction({ title: '删除素材', message: path, confirmation, danger: true });
    if (!action) return;
    try {
        await api(`/api/assets/file?path=${encodeURIComponent(path)}`, { method: 'DELETE', body: JSON.stringify(action) });
        notify('素材已删除');
        await loadAssets(state.assetPage);
    } catch (error) { notify(error.message, true); }
}

async function exportAssets() {
    const action = await requestAction({ title: '导出素材库' });
    if (!action) return;
    try { await download(`/api/assets/export?reason=${encodeURIComponent(action.reason)}`, 'luo-realm-assets.zip'); }
    catch (error) { notify(error.message, true); }
}

async function importAssets(file) {
    const action = await requestAction({ title: '导入素材包', message: '同路径素材将被覆盖。', confirmation: 'assets:import', danger: true });
    if (!action) return;
    try {
        const query = new URLSearchParams({ reason: action.reason, confirm: action.confirm });
        const result = await api(`/api/assets/import?${query}`, { method: 'POST', body: await file.arrayBuffer(), headers: { 'Content-Type': 'application/zip' } });
        notify(`已导入 ${result.imported} 个素材，覆盖 ${result.replaced} 个`);
        await loadAssets(1);
    } catch (error) { notify(error.message, true); }
}

async function exportData() {
    const action = await requestAction({ title: '导出数据库快照' });
    if (!action) return;
    try { await download(`/api/data/export?reason=${encodeURIComponent(action.reason)}`, 'luo-realm.sqlite3'); }
    catch (error) { notify(error.message, true); }
}

async function importData(file) {
    const action = await requestAction({ title: '恢复数据库快照', message: '当前数据会先自动备份，然后被上传快照覆盖。', confirmation: 'database:import', danger: true });
    if (!action) return;
    try {
        const query = new URLSearchParams({ reason: action.reason, confirm: action.confirm });
        const result = await api(`/api/data/import?${query}`, { method: 'POST', body: await file.arrayBuffer(), headers: { 'Content-Type': 'application/vnd.sqlite3' } });
        notify(`数据库已恢复，旧数据备份为 ${result.backup}`);
        await loadDashboard();
    } catch (error) { notify(error.message, true); }
}

async function createBackup() {
    const action = await requestAction({ title: '创建服务器备份' });
    if (!action) return;
    try {
        const result = await api('/api/backup', { method: 'POST', body: JSON.stringify({ reason: action.reason }) });
        notify(`备份已创建：${result.file}`);
    } catch (error) { notify(error.message, true); }
}

async function loadSettings() {
    state.config = await api('/api/config');
    byId('prefixEnabled').value = String(state.config.command.prefix_enabled);
    byId('prefix').value = state.config.command.prefix;
    byId('battleReportEnabled').value = String(state.config.gameplay.battle_report_enabled);
    byId('asciiFpvEnabled').value = String(state.config.game.ascii_fpv_enabled);
    byId('asciiFpvDomain').value = state.config.game.ascii_fpv_domain;
    byId('rewardPublicKey').value = state.config.game.reward_public_key;
    byId('dailyRedemptionLimit').value = state.config.game.daily_redemption_limit;
    byId('adminIds').value = state.config.admin.admin_ids.join('\n');
    byId('bind').value = state.config.admin.bind;
    byId('port').value = state.config.admin.port;
    const systems = await api('/api/definitions/cultivation');
    byId('systems').replaceChildren(...systems.map(system => {
        const row = element('div', 'definition-row');
        row.append(element('strong', '', `${system.name} · ${system.id}`), element('span', 'muted', system.realms.map(realm => realm.name).join(' → ')));
        return row;
    }));
}

async function saveSettings(event) {
    event.preventDefault();
    const action = await requestAction({ title: '保存运行设置', confirmation: 'config:update' });
    if (!action) return;
    const adminIds = byId('adminIds').value.split(/\s+/).filter(Boolean).map(Number).filter(Number.isSafeInteger);
    const payload = {
        command: { prefix_enabled: byId('prefixEnabled').value === 'true', prefix: byId('prefix').value },
        gameplay: { battle_report_enabled: byId('battleReportEnabled').value === 'true' },
        game: {
            ascii_fpv_enabled: byId('asciiFpvEnabled').value === 'true',
            ascii_fpv_domain: byId('asciiFpvDomain').value.trim(),
            reward_public_key: byId('rewardPublicKey').value.trim(),
            daily_redemption_limit: Number(byId('dailyRedemptionLimit').value),
        },
        admin: { enabled: state.config.admin.enabled, bind: byId('bind').value, port: Number(byId('port').value), admin_ids: adminIds },
        ...action,
    };
    try {
        state.config = await api('/api/config', { method: 'PUT', body: JSON.stringify(payload) });
        notify('运行设置已保存');
    } catch (error) { notify(error.message, true); }
}

function generateToken() {
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    byId('newToken').value = btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    notify('新 Token 已在浏览器本地生成');
}

async function rotateToken() {
    const next = byId('newToken').value;
    const action = await requestAction({ title: '轮换管理 Token', confirmation: 'token:rotate', danger: true });
    if (!action) return;
    try {
        await api('/api/token/rotate', { method: 'POST', body: JSON.stringify({ token: next, ...action }) });
        state.token = next;
        sessionStorage.setItem('lr_admin_token', next);
        byId('newToken').value = '';
        notify('管理 Token 已轮换');
    } catch (error) { notify(error.message, true); }
}

async function loadAudit() {
    const data = await api('/api/audit?limit=100');
    const rows = data.items.map(item => {
        const row = document.createElement('tr');
        tableCell(row, new Date(item.created_at * 1000).toLocaleString());
        tableCell(row, item.action_code);
        tableCell(row, `${item.target_type}:${item.target_id}`);
        tableCell(row, item.reason);
        tableCell(row, item.result);
        return row;
    });
    byId('auditRows').replaceChildren(...rows);
}

const pageLoaders = {
    dashboard: loadDashboard,
    groups: loadGroups,
    players: loadPlayers,
    assets: () => loadAssets(1),
    data: async () => {},
    settings: loadSettings,
    audit: loadAudit,
};

const pageTitles = { dashboard: '概览', groups: '群聊', players: '玩家', assets: '素材库', data: '数据', settings: '设置', audit: '审计' };

async function openPage(name) {
    document.querySelectorAll('.page').forEach(page => page.classList.toggle('active', page.id === name));
    document.querySelectorAll('.navigation button').forEach(button => button.classList.toggle('active', button.dataset.page === name));
    byId('pageTitle').textContent = pageTitles[name];
    try { await pageLoaders[name](); }
    catch (error) { notify(error.message, true); }
}

async function authenticate(candidate) {
    const response = await fetch('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: candidate }) });
    const payload = await response.json();
    if (!response.ok || !payload.ok) throw new Error(payload.error?.message || '登录失败');
    state.token = candidate;
    sessionStorage.setItem('lr_admin_token', candidate);
    byId('login').classList.add('hidden');
    byId('app').classList.remove('hidden');
    await openPage('dashboard');
}

byId('loginForm').addEventListener('submit', async event => {
    event.preventDefault();
    byId('loginError').textContent = '';
    try { await authenticate(byId('token').value); }
    catch (error) { byId('loginError').textContent = error.message; }
});
document.querySelectorAll('.navigation button').forEach(button => button.addEventListener('click', () => openPage(button.dataset.page)));
document.querySelectorAll('[data-refresh]').forEach(button => button.addEventListener('click', () => pageLoaders[button.dataset.refresh]()));
byId('logout').addEventListener('click', () => { sessionStorage.removeItem('lr_admin_token'); location.reload(); });
byId('groupSearchButton').addEventListener('click', loadGroups);
byId('groupAddButton').addEventListener('click', addGroup);
byId('playerSearchButton').addEventListener('click', loadPlayers);
byId('assetSearchButton').addEventListener('click', () => loadAssets(1));
byId('assetCategory').addEventListener('change', () => loadAssets(1));
byId('assetUploadInput').addEventListener('change', event => { const file = event.target.files[0]; if (file) uploadAsset(file); event.target.value = ''; });
byId('assetExportButton').addEventListener('click', exportAssets);
byId('assetImportButton').addEventListener('click', () => byId('assetImportInput').click());
byId('assetImportInput').addEventListener('change', event => { const file = event.target.files[0]; if (file) importAssets(file); event.target.value = ''; });
byId('dataExportButton').addEventListener('click', exportData);
byId('dataImportInput').addEventListener('change', event => { const file = event.target.files[0]; if (file) importData(file); event.target.value = ''; });
byId('backupButton').addEventListener('click', createBackup);
byId('configForm').addEventListener('submit', saveSettings);
byId('generateToken').addEventListener('click', generateToken);
byId('rotateToken').addEventListener('click', rotateToken);
byId('auditRefreshButton').addEventListener('click', loadAudit);

if (state.token) authenticate(state.token).catch(() => sessionStorage.removeItem('lr_admin_token'));
