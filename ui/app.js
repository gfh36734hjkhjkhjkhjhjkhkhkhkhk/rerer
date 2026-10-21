'use strict';
const T = window.__TAURI__;
const $ = s => document.querySelector(s), $$ = s => [...document.querySelectorAll(s)];

// В обычном браузере (предпросмотр) работает имитация, в программе — настоящие команды.
const mock = (() => {
  const st = { engine_on:false, strategy:'general.bat', games:false, ai:false, tg_on:false, autostart:false, tray:true,
    tg:{host:'127.0.0.1',port:1443,secret:'7f3a9c41e0b25d88f6c1a04e93b7d2aa',link:'tg://proxy?server=127.0.0.1&port=1443&secret=dd7f3a9c41e0b25d88f6c1a04e93b7d2aa'},
    // Те же 22 стратегии, что в Flowseal 1.10.4, с описаниями как в engine.rs.
    strategies:['Основная','ALT',...Array.from({ length: 12 }, (_, i) => `ALT${i + 2}`),'EXP','FAKE TLS AUTO','FAKE TLS AUTO ALT','FAKE TLS AUTO ALT2','FAKE TLS AUTO ALT3','SIMPLE FAKE','SIMPLE FAKE ALT','SIMPLE FAKE ALT2'].map(name => ({
      id: name === 'Основная' ? 'general.bat' : `general (${name}).bat`, name,
      desc: name === 'Основная' ? 'Рекомендуемая, начните с неё' : name.includes('FAKE TLS') ? 'Поддельное TLS-приветствие, часто помогает на мобильном интернете'
        : name.includes('SIMPLE FAKE') ? 'Простые фейковые пакеты' : name === 'EXP' ? 'Экспериментальная' : 'Альтернативная разрезка пакетов' })),
    since:0, user_sites:['rutracker.org','nnmclub.to'], builtin_sites:142, auto_update:true, app_version:'0.12.0', zapret_version:'1.10.4', portable:false, versions:['zapret-discord-youtube 1.10.4 (winws из zapret v72.9)','tg-ws-proxy 1.11.2'] };
  const diag = [
    { id:'bypass', level:'bad', title:'Установлены другие обходы: GoodbyeDPI', detail:'Они перехватывают тот же трафик, что и Prism, и обход перестаёт работать. Prism может удалить их службы.', fix:true },
    { id:'vpn', level:'warn', title:'Работает VPN: OpenVPN Interactive Service', detail:'Некоторые VPN мешают обходу. Если что-то не открывается, выключите VPN.', fix:false },
    { id:'bfe', level:'ok', title:'Служба базовой фильтрации работает', detail:'', fix:false },
    { id:'files', level:'ok', title:'Файлы обхода на месте', detail:'', fix:false },
    { id:'software', level:'ok', title:'Конфликтующих программ не найдено', detail:'', fix:false },
    { id:'adguard', level:'ok', title:'Adguard не мешает', detail:'', fix:false },
    { id:'proxy', level:'ok', title:'Системный прокси выключен', detail:'', fix:false },
    { id:'hosts', level:'ok', title:'В файле hosts нет записей для YouTube', detail:'', fix:false },
  ];
  const wait = ms => new Promise(r => setTimeout(r, ms));
  return async (cmd, a = {}) => {
    await wait(300);
    switch (cmd) {
      case 'set_engine': await wait(a.on ? 1200 : 300); st.engine_on = a.on; st.since = a.on ? Date.now() - 2 * 3600e3 - 14 * 60e3 : 0; break;
      case 'set_strategy': st.strategy = a.id; break;
      case 'set_games': st.games = a.on; break;
      case 'set_ai': st.ai = a.on; break;
      case 'set_tg': st.tg_on = a.on; break;
      case 'set_autostart': st.autostart = a.on; break;
      case 'set_tray': st.tray = a.on; break;
      case 'add_site': {
        const list = a.domain.split(/[\s,;]+/).filter(Boolean).map(s => s.toLowerCase().replace(/^https?:\/\//, '').split(/[/?#:]/)[0].replace(/^www\./, ''));
        const fresh = list.filter(s => !st.user_sites.includes(s));
        if (!fresh.length) throw `${list[0]} уже в списке`;
        st.user_sites.unshift(...fresh); break;
      }
      case 'remove_site': st.user_sites = st.user_sites.filter(s => s !== a.domain); break;
      case 'check_services': await wait(900); return { discord:{ok:st.engine_on,ms:120,good:2,total:2,failed:[]}, youtube:{ok:st.engine_on,ms:140,good:2,total:3,failed:['видеосервер']}, telegram:{ok:st.tg_on,ms:1,good:0,total:0,failed:[]}, chatgpt:st.ai };
      case 'auto_pick':
        for (const [i, s] of st.strategies.entries()) { emit('pick-progress',{id:s.id,state:'wait',ms:0}); await wait(170); emit('pick-progress',{id:s.id,state:/ALT[259]\)/.test(s.id)?'bad':s.id.includes('SIMPLE')?'part':'ok',ms:90+(i*47)%170}); }
        st.strategy = 'general.bat'; st.engine_on = true; st.since = Date.now(); break;
      case 'get_log': return ['12:00:01 [prism] Prism 0.12.0 запущен', '12:00:01 [prism] установка файлов zapret flowseal-1.10.4', '12:00:02 [prism] запуск стратегии general.bat, игровой режим: false', '12:00:02 [winws] windivert initialized. capture is started.', '12:00:10 [prism] автообновление обходов: Не удалось проверить обновления: тайм-аут', '12:00:12 [prism] предпросмотр в браузере'].join('\n');
      case 'set_auto_update': st.auto_update = a.on; break;
      case 'check_updates': return { app:{current:'0.12.0',latest:'0.12.1',available:true}, zapret:{current:'1.10.4',latest:'1.10.4',available:false} };
      case 'update_zapret': throw 'У вас уже последняя версия обходов';
      case 'update_app': return null;
      case 'run_diagnostics': await wait(1200); return JSON.parse(JSON.stringify(diag));
      case 'fix_problem': await wait(900); Object.assign(diag[0], { level:'ok', title:'Других обходов не найдено', detail:'', fix:false }); return JSON.parse(JSON.stringify(diag));
      case 'open_tg_link': case 'close_window': case 'open_data_dir': return null;
    }
    return JSON.parse(JSON.stringify(st));
  };
})();
const handlers = {};
function emit(ev, payload) { (handlers[ev] || []).forEach(f => f({ payload })); }
const invoke = T ? T.core.invoke : mock;
const listen = T ? T.event.listen : async (ev, f) => { (handlers[ev] ||= []).push(f); };


let S = null;          // последнее состояние из программы
let svc = null;        // результаты последней проверки Discord и YouTube
let checkedAt = 0;     // когда была последняя проверка
let onSince = 0;       // запасной вариант, если программа не прислала время включения
let busy = null;       // обход включается или выключается: {text, target}
let checkGen = 0;      // поколение проверки: старые ответы после переключений не показываем
const invalidate = () => { checkGen++; svc = null; };
let picking = false;   // идёт подбор стратегии
const pick = {};       // результаты подбора по id стратегии

// Длинные сообщения (например, почему не подобралась стратегия) висят дольше.
const toast = t => { const el = $('#toast'); el.textContent = t; el.classList.add('show'); clearTimeout(toast.t); toast.t = setTimeout(() => el.classList.remove('show'), Math.max(2800, String(t).length * 60)); };
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;' }[c]));
const both = a => a.length > 1 ? a.slice(0, -1).join(', ') + ' и ' + a[a.length - 1] : a[0];
// 1 сайт, 2 сайта, 5 сайтов
const plural = (n, f) => f[n % 10 === 1 && n % 100 !== 11 ? 0 : n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 10 || n % 100 >= 20) ? 1 : 2];

async function call(cmd, args, okText) {
  try {
    const r = await invoke(cmd, args);
    if (r && typeof r === 'object' && 'strategies' in r) { S = r; render(); }
    if (okText) toast(okText);
    return r;
  } catch (e) {
    toast(String(e));
    await refresh();
    throw e;
  }
}
async function refresh() { S = await invoke('get_state'); render(); }

// Окно
$('#wMin').onclick = () => T && T.window.getCurrentWindow().minimize();
$('#wClose').onclick = () => invoke('close_window');

// Разделы: ползунок плавно переезжает к выбранной вкладке
function ink() {
  const nav = $('.seg'), cur = $('.seg button[aria-current=page]'), line = $('.seg-ink');
  if (!nav || !cur || !line) return;
  line.style.setProperty('--x', cur.offsetLeft + 'px');
  line.style.width = cur.offsetWidth + 'px';
}
function go(page) {
  $$('.seg button').forEach(b => b.dataset.go === page ? b.setAttribute('aria-current', 'page') : b.removeAttribute('aria-current'));
  $$('.page').forEach(p => p.classList.toggle('on', p.id === 'p-' + page));
  ink();
  if (page === 'set') loadLog();
}
$$('.seg button').forEach(b => b.onclick = () => go(b.dataset.go));
addEventListener('resize', ink);
document.fonts && document.fonts.ready.then(ink);
ink();

// Главный переключатель
const powerBtn = $('#power');
powerBtn.addEventListener('pointerdown', e => {
  const r = powerBtn.getBoundingClientRect();
  const x = e.clientX - r.left, y = e.clientY - r.top;
  const d = 2 * Math.hypot(Math.max(x, r.width - x), Math.max(y, r.height - y));
  const el = document.createElement('span');
  el.className = 'ripple';
  el.style.cssText = `left:${x - d / 2}px;top:${y - d / 2}px;width:${d}px;height:${d}px`;
  powerBtn.appendChild(el);
  el.classList.add('go');
  setTimeout(() => el.remove(), 650);
});
powerBtn.onclick = async () => {
  if (busy || picking || !S) return;
  const on = !S.engine_on;
  busy = { text: on ? 'Включаю…' : 'Выключаю…', target: on }; invalidate(); render();
  try { await call('set_engine', { on }, on ? 'Обход включён' : 'Обход выключен'); } catch {}
  busy = null; render(); checkSoon();
};

// Остальные переключатели
const modCmd = { tg:'set_tg', games:'set_games', ai:'set_ai', autostart:'set_autostart', tray:'set_tray', auto_update:'set_auto_update' };
const modText = {
  tg: on => on ? 'Прокси для Telegram включён' : 'Прокси для Telegram выключен',
  games: on => on ? 'Режим для игр включён' : 'Режим для игр выключен',
  ai: on => on ? 'Доступ к нейросетям включён' : 'Доступ к нейросетям выключен',
  autostart: on => on ? 'Автозапуск включён' : 'Автозапуск выключен',
  tray: on => on ? 'Крестик будет сворачивать в трей' : 'Крестик будет закрывать Prism',
  auto_update: on => on ? 'Автообновление включено' : 'Автообновление выключено',
};
const modVal = { tg: () => S.tg_on, games: () => S.games, ai: () => S.ai, autostart: () => S.autostart, tray: () => S.tray, auto_update: () => S.auto_update };
$$('.sw[data-mod]').forEach(sw => sw.onclick = async () => {
  if (!S) return;
  const m = sw.dataset.mod, on = !modVal[m]();
  sw.disabled = true; sw.setAttribute('aria-checked', on);
  if (m === 'tg' && svc) delete svc.telegram;
  try { await call(modCmd[m], { on }, modText[m](on)); } catch {}
  sw.disabled = false; render();
  if (m === 'tg') checkSoon();
});

// Telegram
$$('[data-copy]').forEach(b => b.onclick = () => { navigator.clipboard.writeText($('#' + b.dataset.copy).textContent); toast('Скопировано'); });
$('#tgCopyLink').onclick = () => { navigator.clipboard.writeText(S.tg.link); toast('Ссылка на прокси скопирована'); };
$('#tgOpen').onclick = async () => {
  try { await invoke('open_tg_link'); toast('Telegram предложит подключить прокси. Нажмите «Подключить»'); }
  catch (e) { toast(String(e)); }
};
$('#tgData').onclick = e => {
  const kv = $('#tgKv'), open = kv.hidden;
  kv.hidden = !open; e.currentTarget.setAttribute('aria-expanded', open);
  e.currentTarget.textContent = open ? 'Скрыть данные' : 'Данные для ручной настройки';
};

// Стратегии: подбор на главной, ручной выбор в отдельном окне
const dlg = $('#stratDlg');
// Выбранная стратегия может быть в конце длинного списка: сразу показываем её.
const showSelected = () => { const o = $('#strats .opt.sel'); if (o) o.scrollIntoView({ block: 'center' }); };
$('#stratOpen').onclick = () => { drawStrats(); dlg.showModal(); showSelected(); };
$('#dlgClose').onclick = () => dlg.close();
dlg.onclick = e => { if (e.target === dlg) dlg.close(); };
function drawStrats() {
  const opt = (s, solo) => {
    const r = pick[s.id];
    const res = !r ? '' : r.state === 'wait' ? '<span class="res wait">Проверяю…</span>'
      : r.state === 'ok' ? `<span class="res ok">${r.ms} мс</span>`
      : r.state === 'part' ? '<span class="res">Частично</span>' : '<span class="res bad">Не работает</span>';
    const sel = s.id === S.strategy;
    return `<div class="opt${sel ? ' sel' : ''}${r && r.state === 'wait' ? ' now' : ''}" data-id="${esc(s.id)}" role="radio" tabindex="0" aria-checked="${sel}"><span class="radio"></span><div class="t"><b>${esc(s.name)}</b>${solo ? `<span>${esc(s.desc)}</span>` : ''}</div>${res}</div>`;
  };
  // У стратегий одного семейства одинаковое описание: показываем его один раз над группой.
  const groups = [];
  for (const s of S.strategies) {
    const g = groups.find(x => x.desc === s.desc);
    if (g) g.items.push(s); else groups.push({ desc: s.desc, items: [s] });
  }
  $('#strats').innerHTML = groups.map(g => g.items.length === 1 ? opt(g.items[0], true)
    : `<div class="og" role="group" aria-label="${esc(g.desc)}"><div class="og-h">${esc(g.desc)}<i>${g.items.length}</i></div>${g.items.map(s => opt(s, false)).join('')}</div>`).join('')
    || '<p class="dlg-sub">Стратегии не найдены. Переустановите Prism.</p>';
  $$('#strats .opt').forEach(o => o.onclick = o.onkeydown = async e => {
    if (e.key && e.key !== 'Enter' && e.key !== ' ') return;
    if (e.key) e.preventDefault();
    if (picking) return;
    const s = S.strategies.find(x => x.id === o.dataset.id);
    invalidate(); dlg.close();
    try { await call('set_strategy', { id: s.id }, `Стратегия «${s.name}» ${S.engine_on ? 'включена' : 'выбрана'}`); } catch {}
    checkSoon();
  });
  // Во время подбора проверяемая стратегия остаётся на виду.
  const now = picking && $('#strats .opt.now');
  if (now) now.scrollIntoView({ block: 'nearest' });
}
listen('pick-progress', e => { pick[e.payload.id] = e.payload; render(); });
async function autoPick() {
  if (picking || busy || !S) return;
  picking = true;
  for (const k in pick) delete pick[k];
  render();
  try {
    await call('auto_pick');
    const s = S.strategies.find(x => x.id === S.strategy);
    toast(`Подобрана стратегия «${s ? s.name : S.strategy}». Обход включён`);
  } catch {}
  picking = false; invalidate(); render(); checkSoon();
  if (dlg.open) showSelected();
}
$('#pickBtn').onclick = autoPick;
// Подбор можно запустить и из окна стратегий: результаты видны там же по ходу проверки.
$('#dlgPick').onclick = autoPick;

// Сайты
function drawSites() {
  const list = S.user_sites;
  $('#sites').innerHTML = list.map(s => `<div class="row"><span class="mono" aria-hidden="true">${esc(s[0])}</span><div class="t"><b>${esc(s)}</b></div><button class="x" data-del="${esc(s)}" aria-label="Убрать ${esc(s)}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6 6 18"/></svg></button></div>`).join('')
    || '<div class="empty"><span class="ic" aria-hidden="true"><svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></svg></span><div class="t"><b>Здесь пока пусто</b><span>Добавьте сайт, который не открывается</span></div></div>';
  $('#builtin').textContent = `Ещё ${S.builtin_sites} сайтов уже есть во встроенном списке: Discord, YouTube и другие.`;
  $$('[data-del]').forEach(b => b.onclick = () => call('remove_site', { domain: b.dataset.del }, `${b.dataset.del} убран из списка`).catch(() => {}));
}
$('#addForm').onsubmit = async e => {
  e.preventDefault();
  const v = $('#addInput').value.trim();
  if (!v) return;
  const before = new Set(S.user_sites);
  try {
    await call('add_site', { domain: v });
    const added = S.user_sites.filter(s => !before.has(s));
    toast(added.length === 1 ? `${added[0]} добавлен в список` : `Добавлено ${added.length} ${plural(added.length, ['сайт', 'сайта', 'сайтов'])}`);
    $('#addInput').value = '';
  } catch {}
};

// Журнал: новые строки внизу, поэтому держим прокрутку у конца, если её не сдвигали
// Строка журнала: время и источник приглушены, сбои выделены целиком.
function logLine(l) {
  const m = l.match(/^(\d\d:\d\d:\d\d) (\[[^\]]+\] )?(.*)$/);
  if (!m) return esc(l);
  const bad = /не удалось|не запустил|ошибк|неожиданно|завершился с кодом|больше не перезапускается/i.test(m[3]);
  return `<span${bad ? ' class="le"' : ''}><span class="lt">${m[1]}</span> ${m[2] ? `<span class="lg">${esc(m[2])}</span>` : ''}${esc(m[3])}</span>`;
}
async function loadLog() {
  try {
    const el = $('#log'), atEnd = el.scrollTop + el.clientHeight >= el.scrollHeight - 24;
    const text = await invoke('get_log');
    el.innerHTML = text ? text.split('\n').map(logLine).join('\n') : 'Пока пусто';
    if (atEnd) el.scrollTop = el.scrollHeight;
  } catch {}
}
$('#logRefresh').onclick = loadLog;
$('#logCopy').onclick = () => { navigator.clipboard.writeText($('#log').textContent); toast('Журнал скопирован'); };
$('#logFolder').onclick = () => invoke('open_data_dir').catch(e => toast(String(e)));

// Диагностика: что мешает обходу на этом компьютере
let D = null;
function drawDiag() {
  $$('#diag .d').forEach(el => el.remove());
  if (!D) return;
  const issues = D.filter(i => i.level === 'bad' || i.level === 'warn').length;
  $('#diagSub').textContent = issues ? `${plural(issues, ['Нашлась', 'Нашлись', 'Нашлось'])} ${issues} ${plural(issues, ['проблема', 'проблемы', 'проблем'])}. Начните с первой` : 'Проблем не найдено. Если обход всё равно не работает, нажмите «Подобрать» на главной';
  $('#diag').insertAdjacentHTML('beforeend', D.map(i => `<div class="item d ${esc(i.level)}"><div class="row"><i class="mark" aria-hidden="true"></i><div class="t"><b>${esc(i.title)}</b>${i.detail ? `<span>${esc(i.detail)}</span>` : ''}</div>${i.fix && i.level !== 'ok' ? `<button class="btn${i.level === 'bad' ? ' primary' : ''}" data-fix="${esc(i.id)}">Исправить</button>` : ''}</div></div>`).join(''));
  $$('[data-fix]').forEach(b => b.onclick = () => fixProblem(b));
}
$('#diagRun').onclick = async () => {
  const b = $('#diagRun');
  b.disabled = true; b.textContent = 'Проверяю…';
  try { D = await invoke('run_diagnostics'); drawDiag(); } catch (e) { toast(String(e)); }
  b.disabled = false; b.textContent = 'Проверить снова';
};
async function fixProblem(b) {
  const id = b.dataset.fix;
  // Удаление служб других обходов не отменить, поэтому нужно второе нажатие.
  if (id === 'bypass' && !b.dataset.sure) { b.dataset.sure = '1'; b.textContent = 'Удалить их службы?'; return; }
  $$('[data-fix]').forEach(x => x.disabled = true);
  b.textContent = 'Исправляю…';
  try { D = await invoke('fix_problem', { id }); toast('Готово'); await refresh(); invalidate(); checkSoon(); } catch (e) { toast(String(e)); }
  drawDiag();
}

// Проверка Discord и YouTube
let checkTimer = null;
function checkSoon() { clearTimeout(checkTimer); checkTimer = setTimeout(checkServices, 700); }
async function checkServices() {
  if (picking || busy) return;
  const gen = checkGen, rb = $('#recheck');
  rb.classList.add('spin'); rb.disabled = true;
  try {
    const r = await invoke('check_services');
    if (gen === checkGen && !busy && !picking) { svc = r; checkedAt = Date.now(); }
  } catch {}
  rb.classList.remove('spin'); rb.disabled = false;
  render();
}
$('#recheck').onclick = () => { invalidate(); render(); checkServices(); };
setInterval(() => { if (!document.hidden) checkServices(); }, 60000);

// Проверка Discord и YouTube. Для YouTube важно не только «сайт открылся»,
// но и сколько адресов ответило: видео живёт на отдельном googlevideo.com.
function web(k) {
  if (!S.engine_on) return ['off', 'Выключено', 0];
  if (picking) return ['wait', 'Проверяю…', 0];
  const x = svc && svc[k];
  if (!x) return ['wait', 'Проверяю…', 0];
  if (!x.ok) return ['fail', 'Не открывается', 0];
  if (x.total > 1 && x.good < x.total) {
    const what = (x.failed || []).includes('видеосервер') ? 'видео не грузится' : `${x.good} из ${x.total}`;
    return ['part', `${x.ms} мс, ${what}`, x.ms < 150 ? 3 : 2];
  }
  return ['ok', `${x.ms} мс`, x.ms < 150 ? 3 : x.ms < 400 ? 2 : 1];
}
const ago = t => { const m = Math.floor((Date.now() - t) / 60000); return m < 1 ? 'только что' : m < 60 ? `${m} мин назад` : `${Math.floor(m / 60)} ч назад`; };
function dur(ms) {
  const m = Math.floor(ms / 60000), h = Math.floor(m / 60);
  return m < 1 ? 'меньше минуты' : h ? `${h} ч ${m % 60} мин` : `${m} мин`;
}
// Время работы и давность проверки обновляются сами, без перерисовки всего окна.
function drawClock() {
  if (!S) return;
  const on = S.engine_on && !busy && !picking;
  const since = S.since || onSince;
  $('#uptime').textContent = on && since ? `Работает ${dur(Date.now() - since)}` : '';
  $('#checked').textContent = S.engine_on && checkedAt && !picking ? ago(checkedAt) : '';
}
setInterval(drawClock, 15000);

function render() {
  if (!S) return;
  const busyText = busy && busy.text, target = busy && busy.target;
  const on = S.engine_on;
  if (on && !onSince) onSince = Date.now();
  if (!on) onSince = 0;
  document.body.classList.toggle('on', busyText ? !!target : on);
  document.body.classList.toggle('busy', !!busyText);
  document.body.classList.toggle('pick', picking);
  const pw = $('#power');
  pw.setAttribute('aria-checked', busyText ? !!target : on);
  pw.disabled = busy || picking;

  // Состояние и подсказка
  const dc = web('discord'), yt = web('youtube');
  const failed = [['Discord', dc], ['YouTube', yt]].filter(([, x]) => x[0] === 'fail').map(([n]) => n);
  const partial = [['Discord', dc], ['YouTube', yt]].filter(([, x]) => x[0] === 'part').map(([n]) => n);
  const sub = $('#stateSub');
  const total = S.strategies.length, done = Object.values(pick).filter(r => r.state !== 'wait').length;
  $('#state').textContent = busyText || (picking ? 'Подбираю стратегию' : on ? 'Обход включён' : 'Обход выключен');
  sub.className = 'state-sub' + ((failed.length || partial.length) && !busyText && !picking ? ' bad' : '');
  sub.textContent = busyText ? 'Это займёт пару секунд.'
    : picking ? `Проверяю ${Math.min(done + 1, total)} из ${total}. Это займёт пару минут.`
    : !on ? 'Нажмите, чтобы открылись Discord и YouTube.'
    : failed.length ? `${both(failed)} не ${failed.length > 1 ? 'открываются' : 'открывается'}. Нажмите «Подобрать», чтобы найти рабочую стратегию.`
    : partial.length ? `${both(partial)}: открывается не всё. Нажмите «Подобрать», чтобы проверить другие стратегии.`
    : dc[0] === 'wait' || yt[0] === 'wait' ? 'Проверяю Discord и YouTube…'
    : 'Discord и YouTube открываются.';
  for (const [id, x] of [['dc', dc], ['yt', yt]]) {
    const row = $(`#svc-${id}`);
    row.className = 'row svc ' + x[0];
    row.dataset.q = x[2];
    $(`#st-${id}`).textContent = x[1];
  }
  $('#recheck').hidden = !on || busyText || picking;

  // Стратегия и подбор
  const strat = S.strategies.find(x => x.id === S.strategy);
  const cur = S.strategies.find(x => pick[x.id] && pick[x.id].state === 'wait');
  $('#stratName').textContent = !total ? 'Не найдены. Переустановите Prism'
    : picking ? (cur ? `Проверяю «${cur.name}»` : 'Готовлю проверку') : (strat ? strat.name : S.strategy);
  $('#arc').style.strokeDasharray = picking ? `${Math.max(2, Math.round(done / Math.max(total, 1) * 100))} 100` : '';
  const pb = $('#pickBtn');
  pb.textContent = picking ? 'Подбираю…' : 'Подобрать';
  pb.className = 'btn' + (failed.length && !picking ? ' primary' : '');
  pb.disabled = picking || busy || !total;
  // Окно стратегий открывается и во время подбора, чтобы следить за результатами.
  $('#stratOpen').disabled = !total;
  const dp = $('#dlgPick');
  dp.textContent = picking ? 'Подбираю…' : 'Подобрать';
  dp.disabled = picking || busy || !total;
  $('#dlgSub').textContent = picking ? `Проверяю ${Math.min(done + 1, total)} из ${total}. Результаты появляются по ходу проверки.`
    : 'Провайдеры блокируют по-разному. Не знаете, какую выбрать? Prism проверит все и включит самую быструю.';
  if (dlg.open) drawStrats();

  // Модули
  for (const m in modVal) $$(`.sw[data-mod="${m}"]`).forEach(s => { if (!s.disabled) s.setAttribute('aria-checked', modVal[m]()); });
  const tgBad = S.tg_on && svc && svc.telegram && !svc.telegram.ok;
  $('#tgSub').textContent = !S.tg_on ? 'Прокси на вашем компьютере'
    : tgBad ? 'Не запустился. Подробности в журнале в настройках' : `Работает на ${S.tg.host}:${S.tg.port}`;
  $('#tgMore').hidden = !S.tg_on || tgBad;
  if (!S.tg_on) $('#tgKv').hidden = true;
  $('#tgHost').textContent = S.tg.host; $('#tgPort').textContent = S.tg.port; $('#tgSecret').textContent = 'dd' + S.tg.secret;
  $('#gmSub').textContent = !S.games ? 'Онлайн-игры и Discord. Вход в игры не ломает'
    : on ? 'Работает. Лаунчеры и античиты не трогает' : 'Включится вместе с обходом';
  $('#tgItem').classList.toggle('is-on', S.tg_on && !tgBad);
  $('#gmItem').classList.toggle('is-on', S.games);
  $('#aiItem').classList.toggle('is-on', S.ai);
  $('#asRow').classList.toggle('is-on', S.autostart);
  for (const m of ['autostart', 'tray', 'auto_update']) $$(`#p-set .sw[data-mod="${m}"]`).forEach(s => s.parentElement.classList.toggle('is-on', modVal[m]()));
  $$('.asSub').forEach(el => el.textContent = S.autostart ? 'Включится сам при запуске Windows' : 'Включать обход при запуске Windows');
  $('#aiSub').textContent = S.ai ? 'Работают без VPN, даже когда Prism закрыт'
    : 'ChatGPT, Gemini, Claude и другие без VPN';

  drawUpdates();
  drawSites();
  drawClock();
  ink();
}

// Обновления
let U = null;
function drawUpdates() {
  if (!S) return;
  $('#appVer').textContent = `Версия ${S.app_version}` + (U && U.app.available ? `. Доступна ${U.app.latest}` : U ? '. Последняя версия' : '');
  $('#zapVer').textContent = `Версия ${S.zapret_version || 'неизвестна'}` + (U && U.zapret.available ? `. Доступна ${U.zapret.latest}` : U ? '. Последняя версия' : '');
  $('#appUpd').textContent = U && U.app.available ? (S.portable ? 'Скачать' : 'Обновить') : 'Проверить';
  $('#zapUpd').textContent = U && U.zapret.available ? 'Обновить' : 'Проверить';
  const show = !!(U && U.app.available);
  const ub = $('#updBar');
  if (!ub.disabled) ub.hidden = !show;
  if (show && !ub.disabled) $('#updText').textContent = `${S.portable ? 'Скачать' : 'Обновить до'} ${U.app.latest}`;
}
async function checkUpdates(quiet) {
  try { U = await invoke('check_updates'); drawUpdates(); if (!quiet && !U.app.available && !U.zapret.available) toast('Всё обновлено'); }
  catch (e) { if (!quiet) toast(String(e)); }
}
async function updateApp(btn) {
  if (!U || !U.app.available) return checkUpdates(false);
  const label = btn.id === 'updBar' ? $('#updText') : btn;
  btn.disabled = true; label.textContent = S.portable ? 'Открываю…' : 'Скачиваю…';
  try { await invoke('update_app'); toast(S.portable ? 'Скачайте новую версию со страницы релиза' : 'Устанавливаю обновление. Prism перезапустится сам'); }
  catch (e) { toast(String(e)); }
  btn.disabled = false; drawUpdates();
}
$('#appUpd').onclick = e => updateApp(e.currentTarget);
$('#updBar').onclick = e => updateApp(e.currentTarget);
$('#zapUpd').onclick = async e => {
  const b = e.currentTarget;
  if (!U || !U.zapret.available) return checkUpdates(false);
  b.disabled = true; b.textContent = 'Обновляю…';
  try { await call('update_zapret', {}, 'Обходы обновлены'); U.zapret.available = false; U.zapret.current = S.zapret_version; } catch {}
  b.disabled = false; drawUpdates(); invalidate(); checkSoon();
};
listen('updates', e => { U = e.payload; drawUpdates(); });
listen('zapret-updated', e => toast(`Обходы обновлены до версии ${e.payload}`));
listen('toast', e => toast(String(e.payload)));

// Состояние поменялось без участия окна: переключили в трее, обход упал или перезапустился.
// Во время подбора и переключения ответ придёт вместе с результатом команды.
listen('state-changed', async () => {
  if (busy || picking) return;
  const was = S && S.engine_on;
  try { await refresh(); } catch { return; }
  if (S.engine_on !== was) { invalidate(); render(); }
  // Прокси Telegram мог запуститься уже после прошлой проверки, поэтому проверяем всегда.
  checkSoon();
});
refresh().then(checkSoon);
