'use strict';
const T = window.__TAURI__;
const $ = s => document.querySelector(s), $$ = s => [...document.querySelectorAll(s)];

// В обычном браузере (предпросмотр) работает имитация, в программе — настоящие команды.
const mock = (() => {
  const st = { engine_on:false, strategy:'general.bat', games:false, ai:false, tg_on:false, autostart:false, tray:true,
    tg:{host:'127.0.0.1',port:1443,secret:'7f3a9c41e0b25d88f6c1a04e93b7d2aa',link:'tg://proxy?server=127.0.0.1&port=1443&secret=dd7f3a9c41e0b25d88f6c1a04e93b7d2aa'},
    strategies:[['general.bat','Основная','Рекомендуемая, начните с неё'],['general (ALT).bat','ALT','Альтернативная разрезка пакетов'],['general (ALT2).bat','ALT2','Альтернативная разрезка пакетов'],['general (FAKE TLS AUTO).bat','FAKE TLS AUTO','Поддельное TLS-приветствие, часто помогает на мобильном интернете'],['general (SIMPLE FAKE).bat','SIMPLE FAKE','Простые фейковые пакеты']].map(([id,name,desc])=>({id,name,desc})),
    user_sites:['rutracker.org'], builtin_sites:142, auto_update:true, app_version:'0.2.1', zapret_version:'1.10.3', portable:false, versions:['zapret-discord-youtube 1.10.3 (winws из zapret v72.9)','tg-ws-proxy 1.11.1'] };
  const wait = ms => new Promise(r => setTimeout(r, ms));
  return async (cmd, a = {}) => {
    await wait(300);
    switch (cmd) {
      case 'set_engine': st.engine_on = a.on; break;
      case 'set_strategy': st.strategy = a.id; break;
      case 'set_games': st.games = a.on; break;
      case 'set_ai': st.ai = a.on; break;
      case 'set_tg': st.tg_on = a.on; break;
      case 'set_autostart': st.autostart = a.on; break;
      case 'set_tray': st.tray = a.on; break;
      case 'add_site': st.user_sites.unshift(a.domain); break;
      case 'remove_site': st.user_sites = st.user_sites.filter(s => s !== a.domain); break;
      case 'check_services': await wait(900); return { discord:{ok:st.engine_on,ms:120}, youtube:{ok:st.engine_on,ms:140}, telegram:{ok:st.tg_on,ms:1}, chatgpt:st.ai };
      case 'auto_pick':
        for (const s of st.strategies) { emit('pick-progress',{id:s.id,state:'wait',ms:0}); await wait(400); emit('pick-progress',{id:s.id,state:s.id.includes('ALT2')?'bad':'ok',ms:100+Math.round(Math.random()*150)}); }
        st.strategy = 'general.bat'; st.engine_on = true; break;
      case 'get_log': return '[prism] предпросмотр в браузере';
      case 'set_auto_update': st.auto_update = a.on; break;
      case 'check_updates': return { app:{current:'0.2.1',latest:'0.3.0',available:true}, zapret:{current:'1.10.3',latest:'1.10.3',available:false} };
      case 'update_zapret': throw 'У вас уже последняя версия обходов';
      case 'update_app': return null;
      case 'open_tg_link': case 'close_window': return null;
    }
    return JSON.parse(JSON.stringify(st));
  };
})();
const handlers = {};
function emit(ev, payload) { (handlers[ev] || []).forEach(f => f({ payload })); }
const invoke = T ? T.core.invoke : mock;
const listen = T ? T.event.listen : async (ev, f) => { (handlers[ev] ||= []).push(f); };

let S = null;          // последнее состояние из программы
let svc = null;        // результаты последней проверки сервисов
let busy = false;      // обход включается или выключается
let picking = false;   // идёт подбор стратегии
const pick = {};       // результаты подбора по id стратегии
const RAYS = ['yt', 'games', 'ai', 'tg', 'dc'];

const toast = t => { const el = $('#toast'); el.textContent = t; el.classList.add('show'); clearTimeout(toast.t); toast.t = setTimeout(() => el.classList.remove('show'), 2800); };
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;' }[c]));

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

// Разделы
function go(page) {
  $$('.nav').forEach(n => n.toggleAttribute('aria-current', n.dataset.go === page));
  $$('.nav[aria-current]').forEach(n => n.setAttribute('aria-current', 'page'));
  $$('.page').forEach(p => p.classList.toggle('on', p.id === 'p-' + page));
  if (page === 'set') loadLog();
  if (page === 'home') fit();
}
$$('.nav').forEach(b => b.onclick = () => go(b.dataset.go));

// Схема на главной подстраивается под размер окна
function fit() {
  const home = $('#p-home');
  if (!home.classList.contains('on')) return;
  const k = Math.max(0.6, Math.min(1.3, (home.clientWidth - 80) / 1000, (home.clientHeight - 60) / 420));
  home.style.setProperty('--k', k.toFixed(3));
}
addEventListener('resize', fit);

// Включение обхода: кнопка и сама призма
async function toggleEngine() {
  if (busy || picking || !S) return;
  const on = !S.engine_on;
  busy = true; svc = null; render(on ? 'Включаю…' : 'Выключаю…');
  try { await call('set_engine', { on }, on ? 'Обход включён' : 'Обход выключен'); } catch {}
  busy = false; render(); checkSoon();
}
$('#orb').onclick = toggleEngine;
$('#power').onclick = toggleEngine;
$('#pickHome').onclick = () => { go('strat'); startPick(); };

// Переключатели
const modCmd = { tg:'set_tg', games:'set_games', ai:'set_ai', autostart:'set_autostart', tray:'set_tray', auto_update:'set_auto_update' };
const modText = {
  tg: on => on ? 'Прокси для Telegram включён' : 'Прокси для Telegram выключен',
  games: on => on ? 'Режим для игр включён' : 'Режим для игр выключен',
  ai: on => on ? 'Доступ к нейросетям включён' : 'Доступ к нейросетям выключен',
  autostart: on => on ? 'Prism будет запускаться вместе с Windows' : 'Prism не будет запускаться вместе с Windows',
  tray: on => on ? 'Крестик будет сворачивать в трей' : 'Крестик будет закрывать Prism',
  auto_update: on => on ? 'Автообновление включено' : 'Автообновление выключено',
};
const modVal = { tg: () => S.tg_on, games: () => S.games, ai: () => S.ai, autostart: () => S.autostart, tray: () => S.tray, auto_update: () => S.auto_update };
$$('.sw[data-mod]').forEach(sw => sw.onclick = async () => {
  if (!S) return;
  const m = sw.dataset.mod, on = !modVal[m]();
  const group = $$(`.sw[data-mod="${m}"]`);
  group.forEach(g => { g.disabled = true; g.setAttribute('aria-checked', on); });
  if (m === 'tg' && svc) delete svc.telegram;
  try { await call(modCmd[m], { on }, modText[m](on)); } catch {}
  group.forEach(g => g.disabled = false); render();
  if (m === 'tg' || m === 'ai') checkSoon();
});

// Telegram
$$('[data-copy]').forEach(b => b.onclick = () => { navigator.clipboard.writeText($('#' + b.dataset.copy).textContent); toast('Скопировано'); });
$('#tgCopyLink').onclick = () => { navigator.clipboard.writeText(S.tg.link); toast('Ссылка на прокси скопирована'); };
$('#tgOpen').onclick = async () => {
  try {
    if (!S.tg_on) await call('set_tg', { on: true });
    await invoke('open_tg_link');
    toast('Telegram предложит подключить прокси. Нажмите «Подключить»');
  } catch {}
};

// Стратегии
function drawStrats() {
  $('#strats').innerHTML = S.strategies.map(s => {
    const r = pick[s.id];
    const res = !r ? '' : r.state === 'wait' ? '<span class="res wait">Проверяю…</span>'
      : r.state === 'ok' ? `<span class="res ok">${r.ms} мс</span>`
      : r.state === 'part' ? '<span class="res">Частично</span>' : '<span class="res bad">Не работает</span>';
    const sel = s.id === S.strategy;
    return `<div class="row pick ${sel ? 'sel' : ''}" data-id="${esc(s.id)}" role="radio" tabindex="0" aria-checked="${sel}"><span class="radio"></span><div class="t"><b>${esc(s.name)}</b><span>${esc(s.desc)}</span></div>${res}</div>`;
  }).join('') || '<div class="row"><div class="t"><span class="res bad">Стратегии не найдены. Переустановите Prism.</span></div></div>';
  $$('#strats .row[data-id]').forEach(r => r.onclick = r.onkeydown = async e => {
    if (e.key && e.key !== 'Enter' && e.key !== ' ') return;
    if (e.key) e.preventDefault();
    if (picking) return;
    const s = S.strategies.find(x => x.id === r.dataset.id);
    svc = null;
    try { await call('set_strategy', { id: s.id }, `Стратегия «${s.name}» ${S.engine_on ? 'включена' : 'выбрана'}`); } catch {}
    checkSoon();
  });
}
listen('pick-progress', e => { pick[e.payload.id] = e.payload; drawStrats(); });
async function startPick() {
  if (picking || busy) return;
  picking = true; const b = $('#auto'); b.disabled = true; b.textContent = 'Проверяю стратегии…'; render();
  for (const k in pick) delete pick[k];
  try {
    await call('auto_pick');
    const s = S.strategies.find(x => x.id === S.strategy);
    toast(`Подобрана стратегия «${s ? s.name : S.strategy}». Обход включён`);
  } catch {}
  picking = false; svc = null; b.disabled = false; b.textContent = 'Подобрать автоматически'; render(); checkSoon();
}
$('#auto').onclick = startPick;

// Сайты
function drawSites() {
  const list = S.user_sites;
  $('#sites').innerHTML = list.map(s => `<div class="row"><div class="t"><b>${esc(s)}</b></div><button class="x" data-del="${esc(s)}" aria-label="Убрать ${esc(s)}">×</button></div>`).join('')
    || '<div class="row"><div class="t"><span>Здесь пока пусто. Добавьте сайт, который не открывается.</span></div></div>';
  $('#sitesCount').textContent = list.length ? `Ваши сайты: ${list.length}` : 'Ваши сайты';
  $('#builtin').textContent = `Ещё ${S.builtin_sites} сайтов уже есть во встроенном списке: Discord, YouTube и другие.`;
  $$('[data-del]').forEach(b => b.onclick = () => call('remove_site', { domain: b.dataset.del }, `${b.dataset.del} убран из списка`).catch(() => {}));
}
$('#addForm').onsubmit = async e => {
  e.preventDefault();
  const v = $('#addInput').value.trim();
  if (!v) return;
  try { await call('add_site', { domain: v }, `${v} добавлен в список`); $('#addInput').value = ''; } catch {}
};

// Журнал
async function loadLog() { try { $('#log').textContent = (await invoke('get_log')) || 'Пока пусто'; } catch {} }
$('#logRefresh').onclick = loadLog;

// Проверка сервисов
let checkTimer = null;
function checkSoon() { clearTimeout(checkTimer); checkTimer = setTimeout(checkServices, 700); }
async function checkServices() {
  if (picking || busy) return;
  try { svc = await invoke('check_services'); } catch {}
  render();
}
setInterval(() => { if (!document.hidden) checkServices(); }, 60000);

// Состояние каждого луча: [вид, подпись]
function rays() {
  const on = S.engine_on;
  const web = k => {
    if (!on) return ['off', 'Выключено'];
    const x = svc && svc[k];
    if (!x) return ['wait', 'Проверяю…'];
    return x.ok ? ['on', `Работает, ${x.ms} мс`] : ['fail', 'Не открывается'];
  };
  return {
    yt: web('youtube'),
    dc: web('discord'),
    games: !S.games ? ['off', 'Выключено'] : on ? ['on', 'Включено'] : ['off', 'Включится с обходом'],
    ai: S.ai ? ['on', 'Открыты'] : ['off', 'Выключено'],
    tg: !S.tg_on ? ['off', 'Выключено'] : svc && svc.telegram && !svc.telegram.ok ? ['fail', 'Не запустился'] : ['on', 'Прокси работает'],
  };
}

function render(busyText) {
  if (!S) return;
  const on = S.engine_on, strat = S.strategies.find(x => x.id === S.strategy);
  const scene = $('#scene');
  scene.classList.toggle('on', on && !busyText);
  scene.classList.toggle('busy', !!busyText);
  $('#state').textContent = busyText || (on ? 'Обход работает' : 'Обход выключен');
  $('#stateSub').textContent = on
    ? `Стратегия «${strat ? strat.name : S.strategy}». Если что-то не открывается, подберите другую.`
    : 'Включите обход, чтобы открылись Discord и YouTube.';
  const pw = $('#power');
  pw.textContent = on ? 'Выключить обход' : 'Включить обход';
  pw.classList.toggle('primary', !on);
  pw.disabled = $('#orb').disabled = busy || picking;
  $('#pickHome').disabled = busy || picking;
  $('#orb').setAttribute('aria-pressed', on);
  $('#orb').setAttribute('aria-label', on ? 'Выключить обход' : 'Включить обход');

  const R = rays(); let lit = false;
  for (const k of RAYS) {
    const [st, text] = R[k];
    const g = $(`.ray[data-r="${k}"]`);
    g.classList.remove('on', 'wait', 'fail');
    if (st !== 'off') { g.classList.add(st); lit = true; }
    $(`.lbl[data-r="${k}"]`).className = 'lbl ' + st;
    $(`#st-${k}`).textContent = text;
  }
  scene.classList.toggle('lit', lit && !busyText);

  for (const m in modVal) $$(`.sw[data-mod="${m}"]`).forEach(s => { if (!s.disabled) s.setAttribute('aria-checked', modVal[m]()); });
  $('#d-tg').classList.toggle('on', S.tg_on);
  $('#d-games').classList.toggle('on', S.games);
  $('#d-ai').classList.toggle('on', S.ai);
  $('#r-tg').classList.toggle('on', R.tg[0] === 'on');
  $('#r-games').classList.toggle('on', R.games[0] === 'on');
  $('#r-ai').classList.toggle('on', S.ai);
  $('#tgState').textContent = S.tg_on ? `Работает на ${S.tg.host}:${S.tg.port}` : 'Выключен';
  $('#tgHost').textContent = S.tg.host; $('#tgPort').textContent = S.tg.port; $('#tgSecret').textContent = 'dd' + S.tg.secret;
  $('#gmState').textContent = !S.games ? 'Выключен' : on ? 'Работает' : 'Включится вместе с обходом';
  $('#aiState').textContent = S.ai ? 'Включён' : 'Выключен';
  $$('#aiList .res').forEach(b => { b.className = 'res' + (S.ai ? ' ok' : ''); b.textContent = S.ai ? 'Открыт' : 'Выключено'; });
  drawUpdates();
  drawStrats(); drawSites();
}

// Обновления
let U = null;
function drawUpdates() {
  if (!S) return;
  $('#appVer').textContent = `Версия ${S.app_version}` + (U && U.app.available ? `. Доступна ${U.app.latest}` : U ? '. Последняя версия' : '');
  $('#zapVer').textContent = `Версия ${S.zapret_version || 'неизвестна'}` + (U && U.zapret.available ? `. Доступна ${U.zapret.latest}` : U ? '. Последняя версия' : '');
  $('#appUpd').textContent = U && U.app.available ? (S.portable ? 'Скачать' : 'Обновить') : 'Проверить';
  $('#zapUpd').textContent = U && U.zapret.available ? 'Обновить' : 'Проверить';
  const bar = $('#updBar'), show = !!(U && U.app.available);
  bar.hidden = !show;
  if (show) {
    $('#updText').textContent = `Вышла Prism ${U.app.latest}`;
    $('#updBarBtn').textContent = S.portable ? 'Скачать' : 'Обновить';
  }
}
async function checkUpdates(quiet) {
  try { U = await invoke('check_updates'); drawUpdates(); if (!quiet && !U.app.available && !U.zapret.available) toast('Всё обновлено'); }
  catch (e) { if (!quiet) toast(String(e)); }
}
async function updateApp(btn) {
  if (!U || !U.app.available) return checkUpdates(false);
  btn.disabled = true; btn.textContent = S.portable ? 'Открываю…' : 'Скачиваю…';
  try { await invoke('update_app'); toast(S.portable ? 'Скачайте новую версию со страницы релиза' : 'Запускаю установщик'); }
  catch (e) { toast(String(e)); }
  btn.disabled = false; drawUpdates();
}
$('#appUpd').onclick = e => updateApp(e.currentTarget);
$('#updBarBtn').onclick = e => updateApp(e.currentTarget);
$('#zapUpd').onclick = async e => {
  const b = e.currentTarget;
  if (!U || !U.zapret.available) return checkUpdates(false);
  b.disabled = true; b.textContent = 'Обновляю…';
  try { await call('update_zapret', {}, 'Обходы обновлены'); U.zapret.available = false; U.zapret.current = S.zapret_version; } catch {}
  b.disabled = false; drawUpdates(); svc = null; checkSoon();
};
listen('updates', e => { U = e.payload; drawUpdates(); });
listen('zapret-updated', e => toast(`Обходы обновлены до версии ${e.payload}`));

listen('state-changed', () => refresh().then(checkSoon));
fit();
refresh().then(checkSoon);
