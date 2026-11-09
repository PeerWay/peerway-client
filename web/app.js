(() => {
  'use strict';

  const TOKEN = document.querySelector('meta[name="peerway-token"]').content;
  const GUIDE_KEY = 'peerway.guide.dismissed';
  const $ = (sel) => document.querySelector(sel);
  const $$ = (sel) => Array.from(document.querySelectorAll(sel));

  let state = null;
  let selected = [];
  let currentView = 'send';
  let authMode = 'register';
  let cloudData = null;
  let plans = null;
  let activeOffer = null;
  let settingsDirty = false;
  let settingsFilled = false;
  let ws = null;
  let wsRetry = 1000;
  let codeVisible = false;
  let currentCode = '';
  let recipients = [];
  const RECENT_KEY = 'peerway.recentRecipients';
  let recentRecipients = [];
  let trustedCodesVisible = false;
  const HOME_COLS = 12;
  const HOME_GAP = 12;
  const HOME_ROW = 24;
  const HOME_MIN_W = 3;
  const HOME_MIN_H = 5;
  const HOME_DEFAULTS = {
    nearby: { x: 0, y: 0, w: 7, h: 8 },
    recent: { x: 7, y: 0, w: 5, h: 8 },
    send: { x: 0, y: 8, w: 7, h: 25 },
    received: { x: 7, y: 8, w: 5, h: 18 },
  };
  let homeLayout = [];
  let homeCustomizing = false;
  let homeDrag = null;
  let changelogChecked = false;
  let changelogShown = false;
  let toastId = 0;
  let notifications = [];
  let notificationSeq = 0;
  let knownTransferStatus = new Map();
  let consentBusy = false;
  let uploading = false;
  const UPDATE_COOLDOWN_MS = 30000;
  let lastUpdateCheck = { at: 0, result: null, error: null };

  const ACTIVE_STATUSES = new Set(['offered', 'connecting', 'accepted', 'waiting_accept', 'transferring']);
  function hasActiveWork() {
    return uploading || (state?.transfers || []).some((t) => ACTIVE_STATUSES.has(t.status));
  }

  const isEnglish = () => window.PeerWayI18n?.lang === 'en';

  function detectPlatform() {
    const value = (
      navigator.userAgentData?.platform ||
      navigator.platform ||
      navigator.userAgent ||
      ''
    ).toLowerCase();
    if (value.includes('mac')) return 'macos';
    if (value.includes('win')) return 'windows';
    return 'linux';
  }

  document.body.dataset.os = detectPlatform();

  const VIEW_META = {
    send: {
      es: ['Transferir', 'Elige un equipo o escribe su código.'],
      en: ['Transfer', 'Pick a device or enter its code.'],
    },
    receive: {
      es: ['Actividad', 'Solicitudes, historial y descargas.'],
      en: ['Activity', 'Requests, history and downloads.'],
    },
    contacts: {
      es: ['Contactos', 'Códigos de confianza y permisos.'],
      en: ['Contacts', 'Trusted codes and permissions.'],
    },
    cloud: {
      es: ['Nube', 'Archivos disponibles cuando quieras.'],
      en: ['Cloud', 'Files available whenever you want.'],
    },
    settings: {
      es: ['Ajustes', 'Idioma, descargas y privacidad.'],
      en: ['Settings', 'Language, downloads and privacy.'],
    },
  };

  const STATUS_LABEL = {
    connecting: ['Conectando', 'amber'],
    offered: ['Pendiente', 'amber'],
    waiting_accept: ['Esperando aceptación', 'amber'],
    accepted: ['Aceptada', 'teal'],
    transferring: ['Transfiriendo', 'teal'],
    completed: ['Completado', 'green'],
    failed: ['Falló', 'red'],
    declined: ['Rechazado', 'red'],
    canceled: ['Cancelado', ''],
  };

  function escapeHtml(value) {
    return String(value ?? '')
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }

  function icon(id, cls = '') {
    return `<svg class="${cls}"><use href="#i-${id}"/></svg>`;
  }

  function formatBytes(bytes) {
    const n = Number(bytes) || 0;
    if (n < 1024) return `${n} B`;
    const units = ['KB', 'MB', 'GB', 'TB'];
    let value = n / 1024;
    let index = 0;
    while (value >= 1024 && index < units.length - 1) {
      value /= 1024;
      index += 1;
    }
    return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
  }

  function formatDate(ts) {
    if (!ts) return '—';
    try {
      const locale = window.PeerWayI18n?.locale?.() || 'es';
      return new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' }).format(new Date(ts * 1000));
    } catch {
      return '—';
    }
  }

  function normalizeCode(value) {
    let out = '';
    for (const ch of String(value).toUpperCase()) {
      if (' -_.'.includes(ch)) continue;
      if (ch === 'O') out += '0';
      else if (ch === 'I' || ch === 'L') out += '1';
      else if ('0123456789ABCDEFGHJKMNPQRSTVWXYZ'.includes(ch)) out += ch;
      if (out.length === 8) break;
    }
    return out;
  }

  function isTrustedCode(value) {
    const code = normalizeCode(value);
    if (!code) return false;
    return (state?.trusted_codes || []).some((trusted) => normalizeCode(trusted) === code);
  }

  function formatCode(value) {
    const code = normalizeCode(value);
    return code.length > 4 ? `${code.slice(0, 4)}-${code.slice(4)}` : code;
  }

  async function api(path, options = {}) {
    const headers = { 'x-peerway-token': TOKEN, ...(options.headers || {}) };
    if (options.body !== undefined && !(options.body instanceof FormData)) {
      headers['Content-Type'] = 'application/json';
    }
    const response = await fetch(path, { ...options, headers });
    const contentType = response.headers.get('content-type') || '';
    const data = contentType.includes('application/json') ? await response.json() : await response.text();
    if (!response.ok) {
      const message = typeof data === 'object' && data ? (data.error || data.message) : String(data);
      throw new Error(message || `Error ${response.status}`);
    }
    return data;
  }

  function toast(message, level = 'info') {
    const el = document.createElement('div');
    el.className = `toast ${level}`;
    el.textContent = message;
    el.dataset.id = String(toastId++);
    $('#toasts').appendChild(el);
    setTimeout(() => {
      el.style.opacity = '0';
      el.style.transform = 'translateY(6px)';
      el.style.transition = '0.25s ease';
      setTimeout(() => el.remove(), 260);
    }, 4800);
    const all = $$('#toasts .toast');
    if (all.length > 4) all[0].remove();
  }

  function notificationsAllowed(category) {
    if (!state?.notifications_enabled) return false;
    if (category === 'updates' && state.notify_updates === false) return false;
    if (category === 'transfers' && state.notify_transfers === false) return false;
    return true;
  }

  function nativeNotify(title, body) {
    const invoke = tauriInvoke();
    if (typeof invoke !== 'function' || !document.hidden) return;
    invoke('notify_native', { title, body }).catch(() => {});
  }

  function pushNotification(title, body, level = 'info', category = 'general') {
    if (!notificationsAllowed(category)) return;
    nativeNotify(title, body);
    const recent = notifications.some(
      (item) => item.category === category && item.title === title && Date.now() / 1000 - item.at < 60
    );
    if (recent) return;
    notifications.unshift({
      id: ++notificationSeq,
      title,
      body,
      level,
      category,
      at: Math.floor(Date.now() / 1000),
      read: false,
    });
    if (notifications.length > 30) notifications.length = 30;
    renderNotifications();
  }

  function renderNotifications() {
    const badge = $('#notifyBadge');
    const list = $('#notifyList');
    const empty = $('#notifyEmpty');
    if (!badge || !list || !empty) return;
    const unread = notifications.filter((item) => !item.read).length;
    badge.textContent = String(unread);
    badge.classList.toggle('hidden', unread === 0);
    $('#notifyBtn')?.classList.toggle('has-unread', unread > 0);
    list.innerHTML = notifications
      .map(
        (item) => `<div class="notify-item ${item.read ? '' : 'unread'} ${escapeHtml(item.level)}">
          <strong>${escapeHtml(item.title)}</strong>
          <span>${escapeHtml(item.body)}</span>
          <time>${formatDate(item.at)}</time>
        </div>`
      )
      .join('');
    empty.classList.toggle('hidden', notifications.length > 0);
  }

  function toggleNotifyPanel(force) {
    const panel = $('#notifyPanel');
    if (!panel) return;
    const open = force ?? panel.classList.contains('hidden');
    panel.classList.toggle('hidden', !open);
    if (open) {
      notifications.forEach((item) => { item.read = true; });
      renderNotifications();
    }
  }

  function clearNotifications() {
    notifications = [];
    renderNotifications();
  }

  function trackTransferNotifications(s) {
    const transfers = s?.transfers || [];
    for (const transfer of transfers) {
      const previous = knownTransferStatus.get(transfer.id);
      if (previous && previous !== transfer.status) {
        const files = transfer.files?.length || 0;
        const peer = transfer.peer_name || transfer.peer_code || 'PeerWay';
        if (transfer.status === 'completed') {
          pushNotification(
            isEnglish() ? 'Transfer completed' : 'Transferencia completada',
            isEnglish()
              ? `${files} file(s) with ${peer}.`
              : `${files} archivo(s) con ${peer}.`,
            'success',
            'transfers'
          );
        } else if (transfer.status === 'declined') {
          pushNotification(
            isEnglish() ? 'Transfer declined' : 'Transferencia rechazada',
            isEnglish()
              ? `${peer} declined the transfer.`
              : `${peer} rechazó la transferencia.`,
            'warning',
            'transfers'
          );
        } else if (transfer.status === 'failed') {
          pushNotification(
            isEnglish() ? 'Transfer failed' : 'Transferencia fallida',
            transfer.error || (isEnglish()
              ? `Could not complete the transfer with ${peer}.`
              : `No se pudo completar la transferencia con ${peer}.`),
            'error',
            'transfers'
          );
        }
      }
      knownTransferStatus.set(transfer.id, transfer.status);
    }
  }

  function showUpdate(version, security = false, notify = true) {
    const banner = $('#updateBanner');
    banner.classList.toggle('security', !!security);
    $('#updateKind').classList.toggle('hidden', !security);
    $('#updateTitle').textContent = security
      ? (isEnglish() ? 'Security update available.' : 'Actualización de seguridad disponible.')
      : (isEnglish() ? 'Update available.' : 'Actualización disponible.');
    $('#updateText').textContent = security
      ? (isEnglish()
        ? `Version ${version} fixes a security issue. Installing it is recommended.`
        : `La versión ${version} corrige un problema de seguridad. Se recomienda instalarla.`)
      : (isEnglish()
        ? `Version ${version} is available.`
        : `Actualización ${version} disponible.`);
    $('#updateInstallBtn').classList.remove('hidden');
    $('#updateInstallBtn').disabled = false;
    $('#updateInstallBtn').textContent = isEnglish() ? 'Install now' : 'Instalar ahora';
    $('#updateManualBtn').classList.add('hidden');
    $('#updateLaterBtn').textContent = isEnglish() ? 'Later' : 'Más tarde';
    banner.classList.remove('hidden');
    if (notify) {
      pushNotification(
        security
          ? (isEnglish() ? `Security update ${version} available` : `Actualización de seguridad ${version} disponible`)
          : (isEnglish() ? `Version ${version} available` : `Actualización ${version} disponible`),
        isEnglish()
          ? 'Open PeerWay to install it when your transfers are idle.'
          : 'Abre PeerWay para instalarla cuando no tengas transferencias activas.',
        security ? 'warning' : 'info',
        'updates'
      );
    }
  }

  function hideUpdate() {
    $('#updateBanner').classList.add('hidden');
  }

  function showUpdateError(message) {
    const text = message || (isEnglish()
      ? 'The update could not be verified. Download the latest version manually.'
      : 'No se pudo verificar la actualización. Descarga la última versión manualmente.');
    $('#updateBanner').classList.remove('security');
    $('#updateKind').classList.add('hidden');
    $('#updateTitle').textContent = isEnglish() ? 'Update problem.' : 'Problema con la actualización.';
    $('#updateText').textContent = text;
    $('#updateInstallBtn').classList.add('hidden');
    $('#updateManualBtn').classList.remove('hidden');
    $('#updateLaterBtn').textContent = isEnglish() ? 'Close' : 'Cerrar';
    $('#updateBanner').classList.remove('hidden');
    pushNotification(
      isEnglish() ? 'Update failed' : 'No se pudo actualizar',
      text,
      'error',
      'updates'
    );
  }

  function tauriInvoke() {
    const internals = window.__TAURI_INTERNALS__;
    return typeof internals?.invoke === 'function' ? internals.invoke.bind(internals) : null;
  }

  function setupWindowControls() {
    const invoke = tauriInvoke();
    if (typeof invoke !== 'function') return;
    document.body.classList.add('tauri');
    const maxIcon = document.querySelector('#windowMaxIcon use');
    const updateMaxIcon = (maximized) => {
      if (maxIcon) maxIcon.setAttribute('href', maximized ? '#i-window-restore' : '#i-window-max');
    };
    const safeInvoke = (command, ...args) => invoke(command, ...args).catch(() => null);
    $('#windowMin')?.addEventListener('click', () => { safeInvoke('window_minimize'); });
    $('#windowMax')?.addEventListener('click', async () => {
      const maximized = await safeInvoke('window_toggle_maximize');
      if (typeof maximized === 'boolean') updateMaxIcon(maximized);
    });
    $('#windowClose')?.addEventListener('click', () => {
      if (hasActiveWork()) {
        toast(isEnglish()
          ? 'A transfer or upload is in progress. Wait or cancel it before closing.'
          : 'Hay una transferencia o subida en curso. Espera o cancélala antes de cerrar.', 'warning');
        return;
      }
      safeInvoke('window_close');
    });
    $('#windowTitlebar')?.addEventListener('dblclick', async (event) => {
      if (event.target.closest('.window-controls')) return;
      const maximized = await safeInvoke('window_toggle_maximize');
      if (typeof maximized === 'boolean') updateMaxIcon(maximized);
    });
    safeInvoke('window_is_maximized').then((maximized) => {
      if (typeof maximized === 'boolean') updateMaxIcon(maximized);
    });
    window.addEventListener('resize', () => {
      safeInvoke('window_is_maximized').then((maximized) => {
        if (typeof maximized === 'boolean') updateMaxIcon(maximized);
      });
    });
  }

  async function installUpdateFromWindow() {
    const invoke = tauriInvoke();
    if (typeof invoke !== 'function') {
      toast('La instalación se hace desde el menú de la bandeja del sistema.', 'info');
      return;
    }
    const button = $('#updateInstallBtn');
    button.disabled = true;
    button.textContent = 'Instalando actualización…';
    try {
      await invoke('install_update');
      button.disabled = false;
      button.textContent = 'Instalar ahora';
    } catch (error) {
      button.disabled = false;
      button.textContent = isEnglish() ? 'Install now' : 'Instalar ahora';
      const raw = String(error?.message || error || '');
      const signature = /signature|firma/i.test(raw);
      showUpdateError(signature
        ? (isEnglish()
          ? 'The update signature does not match this installation. Download the latest version manually.'
          : 'La firma de la actualización no coincide con esta instalación. Descarga la última versión manualmente.')
        : (isEnglish()
          ? 'The update could not be installed. Download the latest version manually.'
          : 'No se pudo instalar la actualización. Descarga la última versión manualmente.'));
    }
  }

  async function checkForUpdatesFromWindow() {
    const invoke = tauriInvoke();
    if (typeof invoke !== 'function') return;
    const button = $('#checkUpdateBtn');
    const status = $('#updateStatusText');
    const now = Date.now();

    // The cooldown is deliberately quiet: the click never looks broken, but
    // for 30 seconds it repeats the previous answer instead of calling home.
    if (now - lastUpdateCheck.at < UPDATE_COOLDOWN_MS) {
      if (lastUpdateCheck.result) {
        showUpdate(lastUpdateCheck.result.version, lastUpdateCheck.result.security, false);
        if (status) {
          status.textContent = isEnglish()
            ? `Update ${lastUpdateCheck.result.version} available.`
            : `Actualización ${lastUpdateCheck.result.version} disponible.`;
        }
      } else if (lastUpdateCheck.error) {
        if (status) {
          status.textContent = isEnglish()
            ? 'The last check failed. Try again in a few seconds.'
            : 'La última comprobación falló. Inténtalo de nuevo en unos segundos.';
        }
      } else {
        if (status) {
          status.textContent = isEnglish()
            ? 'Already checked a few seconds ago. You are up to date.'
            : 'Ya comprobaste hace unos segundos. Estás al día.';
        }
        toast(isEnglish()
          ? 'You are already up to date.'
          : 'Ya tienes la última versión.', 'info');
      }
      return;
    }

    button.disabled = true;
    const original = button.innerHTML;
    button.textContent = isEnglish() ? 'Checking…' : 'Comprobando…';
    if (status) {
      status.textContent = isEnglish() ? 'Checking for updates…' : 'Buscando actualizaciones…';
    }
    try {
      const result = await invoke('check_updates_now');
      lastUpdateCheck = { at: Date.now(), result: result || null, error: null };
      if (result) {
        showUpdate(result.version, result.security);
        if (status) {
          status.textContent = result.security
            ? (isEnglish()
              ? `Security update ${result.version} available.`
              : `Actualización de seguridad ${result.version} disponible.`)
            : (isEnglish()
              ? `Update ${result.version} available.`
              : `Actualización ${result.version} disponible.`);
        }
      } else {
        hideUpdate();
        if (status) {
          status.textContent = isEnglish()
            ? 'You are using the latest version.'
            : 'Tienes la última versión.';
        }
      }
    } catch (error) {
      lastUpdateCheck = { at: Date.now(), result: null, error: true };
      if (status) {
        status.textContent = isEnglish()
          ? 'Could not check for updates.'
          : 'No se pudo buscar la actualización.';
      }
      showUpdateError(isEnglish()
        ? 'Could not check for updates. You can download the latest version manually.'
        : 'No se pudo buscar la actualización. Puedes descargar la última versión manualmente.');
    } finally {
      button.disabled = false;
      button.innerHTML = original;
    }
  }

  async function persistLanguage(language) {
    if (!state || (language !== 'es' && language !== 'en')) return;
    try {
      await api('/api/settings', {
        method: 'POST',
        body: JSON.stringify({
          device_name: state.device_name || '',
          download_dir: state.download_dir || '',
          auto_accept_trusted: !!state.auto_accept_trusted,
          lan_discovery: !!state.lan_discovery,
          auto_update: state.auto_update !== false,
          language,
        }),
      });
      state.language = language;
      const invoke = tauriInvoke();
      if (typeof invoke === 'function') invoke('refresh_tray').catch(() => {});
    } catch (error) {
      console.warn('No se pudo guardar el idioma de la bandeja', error);
    }
  }

  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      const area = document.createElement('textarea');
      area.value = text;
      document.body.appendChild(area);
      area.select();
      const ok = document.execCommand('copy');
      area.remove();
      return ok;
    }
  }

  function setView(view) {
    currentView = view;
    $$('.nav-item').forEach((button) => button.classList.toggle('active', button.dataset.view === view));
    $$('.view').forEach((section) => section.classList.toggle('hidden', section.id !== `view-${view}`));
    if (view !== 'send' && homeCustomizing) setHomeCustomizing(false);
    $('#customizeBtn')?.classList.toggle('hidden', view !== 'send');
    if (view === 'send') {
      if (!homeCustomizing) homeLayout = readHomeLayout();
      applyHomeGrid();
    }
    const meta = VIEW_META[view] || {
      es: ['PeerWay', ''],
      en: ['PeerWay', ''],
    };
    const [title, subtitle] = isEnglish() ? meta.en : meta.es;
    $('#pageTitle').textContent = title;
    $('#pageSubtitle').textContent = subtitle;
    if (view === 'settings') {
      fillSettings(true);
      loadSessions();
    }
    if (view === 'cloud' && state?.logged_in) loadCloud();
    if (view === 'receive') renderAll(state);
    if (view === 'contacts') renderTrusted(state || {});
  }

  async function loadState() {
    try {
      const next = await api('/api/state');
      state = next;
      renderAll(state);
    } catch (error) {
      toast(`No se pudo leer el estado: ${error.message}`, 'error');
    }
  }

  function renderCode() {
    const value = $('#bigCode');
    if (!value) return;
    value.textContent = codeVisible && currentCode ? currentCode : '••••-••••';
    const button = $('#toggleCode');
    if (button) {
      const label = codeVisible
        ? (isEnglish() ? 'Hide code' : 'Ocultar código')
        : (isEnglish() ? 'Show code' : 'Mostrar código');
      button.title = label;
      button.setAttribute('aria-label', label);
      button.querySelector('use')?.setAttribute('href', codeVisible ? '#i-eye-off' : '#i-eye');
    }
  }

  function renderAll(s) {
    if (!s) return;
    if (s.platform) document.body.dataset.os = s.platform;
    currentCode = s.code || '';
    renderCode();
    $('#rotateCodeBtn')?.classList.toggle('hidden', !!s.logged_in);
    $('#versionText').textContent = s.version || '0.1.24';
    $('#windowVersion').textContent = s.version || '0.1.24';
    $('#aboutVersion').textContent = `v${s.version || '0.1.24'} · ${s.platform || ''}`;
    const settingsVersion = $('#settingsVersion');
    if (settingsVersion) settingsVersion.textContent = `v${s.version || '0.1.24'} · ${s.platform || ''}`;
    $('#tempBanner')?.classList.toggle('hidden', !!s.logged_in);
    $('#accountName').textContent = s.logged_in
      ? (s.display_name || s.username || 'Cuenta')
      : 'Modo temporal';
    $('#accountPlan').textContent = s.logged_in
      ? `${s.plan_label || 'Gratis'} · ${s.quota_bytes > 0 ? formatBytes(s.quota_bytes) + ' de nube' : 'sin nube'}`
      : 'Inicia sesión para conservar tu código';
    $('#serverDot').className = `dot ${s.server_online ? 'online' : 'offline'}`;
    $('#serverLabel').textContent = s.server_online ? 'PeerWay conectado' : 'Sin conexión';
    $('#lanCount').textContent = `${(s.lan_peers || []).length} en la red`;
    $('#setServerStatus').textContent = s.server_online ? 'Conectado' : 'Sin conexión';
    const serviceDot = $('#serviceDot');
    if (serviceDot) {
      serviceDot.className = `dot ${s.server_online ? 'online' : 'offline'}`;
    }
    renderActive(s);
    renderPending(s);
    renderReceived(s);
    renderHistory(s);
    renderLan(s);
    renderSettingsSummary(s);
    renderConsent(s);
    renderCloudAccount(s);
    renderTrusted(s);
    renderHomeLayout();
    if (!changelogChecked) {
      changelogChecked = true;
      maybeShowChangelog(s);
    }
    trackTransferNotifications(s);
    if (activeOffer) {
      const stillOffered = (s.transfers || []).some(
        (t) => t.id === activeOffer.id && t.status === 'offered'
      );
      if (!stillOffered) hideOffer();
    }
    const pending = (s.transfers || []).filter((t) => t.status === 'offered');
    $('#incomingBadge').textContent = String(pending.length);
    $('#incomingBadge').classList.toggle('hidden', pending.length === 0);
    maybeShowOffer();
    if (currentView === 'settings' && !settingsDirty) fillSettings(false);
  }

  function renderActive(s) {
    const active = (s.transfers || []).filter((t) =>
      ['connecting', 'accepted', 'waiting_accept', 'transferring'].includes(t.status)
    );
    const container = $('#activeTransfers');
    container.innerHTML = active.map((t) => {
      const [label, color] = STATUS_LABEL[t.status] || [t.status, ''];
      const percent = t.total_bytes ? Math.min(100, (t.bytes_done / t.total_bytes) * 100) : 0;
      const direction = t.direction === 'send' ? 'send' : 'receive';
      const trusted = isTrustedCode(t.peer_code);
      const trustLabel = isEnglish() ? 'Trust device' : 'Confiar en este equipo';
      return `<article class="transfer-card">
        <div class="transfer-top">
          <div class="transfer-peer">
            <div class="dir-icon ${direction}">${icon(direction === 'send' ? 'arrow-up' : 'arrow-down')}</div>
            <div><strong>${escapeHtml(t.peer_name || 'Equipo desconocido')}</strong><span>${escapeHtml(t.peer_code)}</span></div>
          </div>
          <span class="chip ${color}">${escapeHtml(label)}</span>
        </div>
        <div class="progress"><span style="width:${percent.toFixed(1)}%"></span></div>
        <div class="transfer-meta">
          <span>${formatBytes(t.bytes_done)} / ${formatBytes(t.total_bytes)}</span>
          <span>${escapeHtml(t.channel || 'Buscando ruta…')}</span>
        </div>
        ${t.error ? `<div class="transfer-error">${escapeHtml(t.error)}</div>` : ''}
        <div class="send-actions" style="margin-top:12px;justify-content:flex-end">
          ${trusted ? '' : `<button class="btn ghost small" data-trust-transfer="${escapeHtml(t.id)}" title="${trustLabel}">${icon('shield')} ${isEnglish() ? 'Trust' : 'Confiar'}</button>`}
          <button class="btn ghost small" data-cancel="${escapeHtml(t.id)}">${icon('x')} Cancelar</button>
        </div>
      </article>`;
    }).join('');
    $('#activeEmpty').classList.toggle('hidden', active.length > 0);
    $('#activeSection')?.classList.toggle('hidden', active.length === 0);
  }

  function renderReceived(s) {
    const section = $('#receivedSection');
    const list = $('#receivedFiles');
    if (!section || !list) return;
    const received = (s.transfers || [])
      .filter((t) => t.direction === 'receive' && t.status === 'completed')
      .slice(0, 4);
    section.classList.toggle('hidden', received.length === 0);
    if (!received.length) {
      list.innerHTML = '';
      return;
    }
    const openFileLabel = isEnglish() ? 'Open file' : 'Abrir archivo';
    const openFolderLabel = isEnglish() ? 'Show in folder' : 'Mostrar en carpeta';
    list.innerHTML = received.map((t) => {
      const names = (t.files || []).map((f) => f.name).join(', ');
      return `<div class="received-row">
        <div class="dir-icon receive">${icon('arrow-down')}</div>
        <div class="received-main">
          <strong>${escapeHtml(names || (isEnglish() ? 'Received file' : 'Archivo recibido'))}</strong>
          <span>${escapeHtml(t.peer_name || '')} · ${formatBytes(t.total_bytes)} · ${formatDate(t.finished_at || t.created_at)}</span>
        </div>
        <div class="history-actions">
          <button class="icon-btn" title="${openFileLabel}" aria-label="${openFileLabel}" data-open-file="${escapeHtml(t.id)}">${icon('download')}</button>
          <button class="icon-btn" title="${openFolderLabel}" aria-label="${openFolderLabel}" data-open-transfer="${escapeHtml(t.id)}">${icon('folder')}</button>
        </div>
      </div>`;
    }).join('');
  }

  function pendingOfferRow(t) {
    const from = t.direction === 'receive' ? t.peer_name : t.peer_name;
    return `<div class="offer-row">
      <div class="dir-icon receive">${icon('arrow-down')}</div>
      <div class="offer-row-main"><strong>${escapeHtml(from || 'Equipo')} · ${t.files.length} archivo(s)</strong><span>${formatBytes(t.total_bytes)} · ${escapeHtml(t.channel || '—')}${t.comment ? ` · ${escapeHtml(t.comment)}` : ''}</span></div>
      <div class="offer-row-actions">
        <button class="btn ghost small" data-decline="${escapeHtml(t.id)}" aria-label="Rechazar">${icon('x')}</button>
        <button class="btn primary small" data-accept="${escapeHtml(t.id)}">${icon('check')} Aceptar</button>
      </div>
    </div>`;
  }

  function renderPending(s) {
    const pending = (s.transfers || []).filter((t) => t.status === 'offered');
    $('#pendingOffers').innerHTML = pending.map(pendingOfferRow).join('');
    $('#pendingEmpty').classList.toggle('hidden', pending.length > 0);
  }

  function renderHistory(s) {
    const history = (s.transfers || []).filter((t) => !['offered', 'connecting', 'accepted', 'waiting_accept', 'transferring'].includes(t.status));
    $('#historyCount').textContent = isEnglish()
      ? `${history.length} transfer${history.length === 1 ? '' : 's'}`
      : `${history.length} transferencia${history.length === 1 ? '' : 's'}`;
    $('#historyList').innerHTML = history.map((t) => {
      const [label, color] = STATUS_LABEL[t.status] || [t.status, ''];
      const direction = t.direction === 'send' ? 'send' : 'receive';
      const detail = t.error ? t.error : `${formatBytes(t.total_bytes)} · ${t.channel || '—'}`;
      const openFileLabel = isEnglish() ? 'Open file' : 'Abrir archivo';
      const openFolderLabel = isEnglish() ? 'Show in folder' : 'Mostrar en carpeta';
      const trustLabel = isEnglish() ? 'Trust this device' : 'Añadir a contactos de confianza';
      const trustButton = isTrustedCode(t.peer_code)
        ? ''
        : `<button class="icon-btn" title="${trustLabel}" aria-label="${trustLabel}" data-trust-transfer="${escapeHtml(t.id)}">${icon('shield')}</button>`;
      const action = t.direction === 'receive' && t.status === 'completed'
        ? `<div class="history-actions">
            <button class="icon-btn" title="${openFileLabel}" aria-label="${openFileLabel}" data-open-file="${escapeHtml(t.id)}">${icon('download')}</button>
            <button class="icon-btn" title="${openFolderLabel}" aria-label="${openFolderLabel}" data-open-transfer="${escapeHtml(t.id)}">${icon('folder')}</button>
            ${trustButton}
          </div>`
        : `<div class="history-actions">${trustButton}</div>`;
      const directionLabel = t.direction === 'send'
        ? (isEnglish() ? `To ${t.peer_name} · ${t.peer_code}` : `Para ${t.peer_name} · ${t.peer_code}`)
        : (isEnglish() ? `From ${t.peer_name} · ${t.peer_code}` : `De ${t.peer_name} · ${t.peer_code}`);
      return `<div class="history-row">
        <div class="dir-icon ${direction}">${icon(direction === 'send' ? 'arrow-up' : 'arrow-down')}</div>
        <div class="history-main">
          <strong>${escapeHtml(t.files.map((f) => f.name).join(', ') || (isEnglish() ? 'Transfer' : 'Transferencia'))}</strong>
          <span>${escapeHtml(directionLabel)} · ${escapeHtml(detail)}</span>
        </div>
        <div class="history-side"><span class="chip ${color}">${escapeHtml(label)}</span><time>${formatDate(t.finished_at || t.created_at)}</time></div>
        ${action}
      </div>`;
    }).join('');
    $('#historyEmpty').classList.toggle('hidden', history.length > 0);
  }

  function renderLan(s) {
    const peers = s.lan_peers || [];
    const count = $('#lanPeerCount');
    if (count) count.textContent = String(peers.length);
    const list = $('#lanPeers');
    if (list) {
      list.innerHTML = peers.map((peer) => `
        <button class="lan-peer" data-send-code="${escapeHtml(peer.code)}" data-send-name="${escapeHtml(peer.device_name)}">
          <span class="avatar">${icon('user')}</span>
          <span class="lan-main"><strong>${escapeHtml(peer.device_name)}</strong><span>${escapeHtml(peer.platform)}</span></span>
          <span class="lan-action">${icon('send')} ${isEnglish() ? 'Send' : 'Enviar'}</span>
        </button>`).join('');
    }
    const empty = $('#lanEmpty');
    if (empty) {
      empty.classList.toggle('hidden', peers.length > 0);
      const text = empty.querySelector('span');
      if (text) {
        text.textContent = s.lan_discovery
          ? (isEnglish() ? 'Looking for devices on your network…' : 'Buscando equipos en tu red…')
          : (isEnglish()
            ? 'Enable discovery in Settings to see nearby devices.'
            : 'Activa el descubrimiento en Ajustes para ver equipos cercanos.');
      }
    }
  }

  function syncNotificationControls() {
    const master = $('#setNotifications');
    const disabled = !master?.checked;
    ['setNotifyUpdates', 'setNotifyTransfers'].forEach((id) => {
      const control = document.getElementById(id);
      if (control) control.disabled = disabled;
    });
  }

  function fillSettings(force) {
    if (!state) return;
    if (force) settingsDirty = false;
    if (settingsDirty && !force) return;
    $('#setDevice').value = state.device_name || '';
    $('#setDownload').value = state.download_dir || '';
    $('#setAutoTrust').checked = !!state.auto_accept_trusted;
    $('#setLan').checked = !!state.lan_discovery;
    $('#setAutoUpdate').checked = state.auto_update !== false;
    $('#setChangelog').checked = !state.changelog_disabled;
    $('#setAutostart').checked = state.autostart_enabled === true;
    $('#setTelemetry').checked = state.telemetry_enabled === true;
    $('#setNotifications').checked = state.notifications_enabled !== false;
    $('#setNotifyUpdates').checked = state.notify_updates !== false;
    $('#setNotifyTransfers').checked = state.notify_transfers !== false;
    syncNotificationControls();
    settingsFilled = true;
  }

  function renderSettingsSummary(s) {
    if (!settingsFilled) fillSettings(true);
    $('#setServerStatus').textContent = s.server_online ? 'Conectado' : 'Sin conexión';
    const serviceDot = $('#serviceDot');
    if (serviceDot) {
      serviceDot.className = `dot ${s.server_online ? 'online' : 'offline'}`;
    }
  }

  function sessionTimestamp(session) {
    const raw = session.last_seen_at ?? session.last_used_at ?? session.created_at ?? session.at;
    if (typeof raw === 'number') return raw > 1e12 ? Math.floor(raw / 1000) : raw;
    if (typeof raw === 'string' && raw) {
      const numeric = Number(raw);
      if (Number.isFinite(numeric)) return numeric > 1e12 ? Math.floor(numeric / 1000) : numeric;
      const parsed = Date.parse(raw);
      if (Number.isFinite(parsed)) return Math.floor(parsed / 1000);
    }
    return 0;
  }

  function renderSessions(items) {
    const list = $('#sessionsList');
    const empty = $('#sessionsEmpty');
    if (!list || !empty) return;
    const english = isEnglish();
    const safe = items.filter((session) => session && typeof session === 'object');
    list.innerHTML = safe.map((session) => {
      const device = session.device_name || session.device || session.app || session.client || session.user_agent || '';
      const app = session.app || session.client || session.platform || '';
      const when = formatDate(sessionTimestamp(session));
      const meta = [app && app !== device ? app : '', when !== '—' ? when : ''].filter(Boolean).join(' · ');
      const current = session.current === true || session.is_current === true || session.this_device === true;
      const id = session.id ?? session.session_id ?? '';
      const action = current
        ? `<span class="chip teal">${english ? 'This device' : 'Este equipo'}</span>`
        : id
          ? `<button class="btn ghost small" type="button" data-revoke-session="${escapeHtml(id)}">${english ? 'Revoke' : 'Revocar'}</button>`
          : '';
      return `<div class="session-row">
        <div class="session-main">
          <strong>${escapeHtml(device || (english ? 'Unknown device' : 'Dispositivo desconocido'))}</strong>
          <span>${escapeHtml(meta || '—')}</span>
        </div>
        ${action}
      </div>`;
    }).join('');
    empty.classList.toggle('hidden', safe.length > 0);
    const emptyText = empty.querySelector('span');
    if (emptyText) {
      emptyText.textContent = english ? 'No other sessions.' : 'No hay otras sesiones.';
    }
  }

  async function loadSessions() {
    const list = $('#sessionsList');
    const empty = $('#sessionsEmpty');
    const others = $('#revokeOthers');
    if (!list || !empty) return;
    const english = isEnglish();
    if (!state?.logged_in) {
      list.innerHTML = '';
      empty.classList.remove('hidden');
      const emptyText = empty.querySelector('span');
      if (emptyText) {
        emptyText.textContent = english ? 'Sign in to see your sessions.' : 'Inicia sesión para ver tus sesiones.';
      }
      others?.classList.add('hidden');
      return;
    }
    others?.classList.remove('hidden');
    list.innerHTML = `<div class="empty-mini"><span>${english ? 'Loading sessions…' : 'Cargando sesiones…'}</span></div>`;
    empty.classList.add('hidden');
    try {
      const data = await api('/api/account/sessions');
      const items = Array.isArray(data) ? data : data?.sessions || data?.items || [];
      renderSessions(items);
    } catch (error) {
      renderSessions([]);
      toast(error.message, 'error');
    }
  }

  async function revokeSession(id) {
    try {
      await api('/api/account/sessions/revoke', {
        method: 'POST',
        body: JSON.stringify({ id }),
      });
      toast(isEnglish() ? 'Session revoked.' : 'Sesión revocada.', 'success');
      await loadSessions();
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  async function revokeOtherSessions() {
    if (!confirm(isEnglish() ? 'Sign out all other sessions?' : '¿Cerrar las demás sesiones?')) return;
    const button = $('#revokeOthers');
    if (button) button.disabled = true;
    try {
      await api('/api/account/sessions/revoke-others', { method: 'POST' });
      toast(isEnglish() ? 'Other sessions were signed out.' : 'Las demás sesiones se cerraron.', 'success');
      await loadSessions();
    } catch (error) {
      toast(error.message, 'error');
    } finally {
      if (button) button.disabled = false;
    }
  }

  function renderConsent(s) {
    const overlay = $('#consentOverlay');
    if (!overlay) return;
    const needed = s && s.telemetry_choice_made !== true;
    overlay.classList.toggle('hidden', !needed);
  }

  function showGuide() {
    const overlay = $('#guideOverlay');
    if (!overlay) return;
    overlay.classList.remove('hidden');
    $('#guideClose')?.focus();
  }

  function hideGuide() {
    $('#guideOverlay')?.classList.add('hidden');
    localStorage.setItem(GUIDE_KEY, '1');
  }

  function maybeShowGuide() {
    if (localStorage.getItem(GUIDE_KEY) === '1') return;
    showGuide();
  }

  async function chooseConsent(accepted) {
    if (consentBusy) return;
    consentBusy = true;
    const accept = $('#consentAccept');
    const reject = $('#consentReject');
    if (accept) accept.disabled = true;
    if (reject) reject.disabled = true;
    try {
      await api('/api/consent', {
        method: 'POST',
        body: JSON.stringify({ accepted }),
      });
      $('#consentOverlay').classList.add('hidden');
      toast(accepted
        ? (isEnglish() ? 'Usage statistics enabled.' : 'Estadísticas de uso activadas.')
        : (isEnglish() ? 'Usage statistics disabled.' : 'Estadísticas de uso desactivadas.'),
        'success');
      await loadState();
    } catch (error) {
      toast(error.message, 'error');
    } finally {
      consentBusy = false;
      if (accept) accept.disabled = false;
      if (reject) reject.disabled = false;
    }
  }

  function renderTrusted(s) {
    const codes = s.trusted_codes || [];
    const removeLabel = isEnglish() ? 'Remove' : 'Quitar';
    $('#trustedList').innerHTML = codes.map((code) => {
      const initial = escapeHtml((code.trim()[0] || '?').toUpperCase());
      const shown = trustedCodesVisible ? escapeHtml(code) : '••••-••••';
      const avatar = trustedCodesVisible ? initial : icon('user');
      return `
      <div class="trusted-contact">
        <span class="trusted-avatar" aria-hidden="true">${avatar}</span>
        <div class="trusted-who">
          <strong>${shown}</strong>
          <span>${isEnglish() ? 'Trusted device' : 'Equipo de confianza'}</span>
        </div>
        <button class="icon-btn" data-copy-trusted="${escapeHtml(code)}" title="${isEnglish() ? 'Copy code' : 'Copiar código'}" aria-label="${isEnglish() ? 'Copy code' : 'Copiar código'}">${icon('copy')}</button>
        <button class="icon-btn" data-untrust="${escapeHtml(code)}" title="${removeLabel}" aria-label="${removeLabel}">${icon('x')}</button>
      </div>`;
    }).join('');
    $('#trustedEmpty').classList.toggle('hidden', codes.length > 0);
    const toggle = $('#toggleTrustedCodes');
    if (toggle) {
      $('#toggleTrustedLabel').textContent = trustedCodesVisible
        ? (isEnglish() ? 'Hide codes' : 'Ocultar códigos')
        : (isEnglish() ? 'Show codes' : 'Mostrar códigos');
      toggle.querySelector('use')?.setAttribute('href', trustedCodesVisible ? '#i-eye-off' : '#i-eye');
    }
  }

  // ---- Home layout (customization mode) ----

  function homeWidgets() {
    const grid = $('#homeGrid');
    return grid ? Array.from(grid.querySelectorAll('[data-widget]')) : [];
  }

  function defaultHomeLayout() {
    return Object.entries(HOME_DEFAULTS).map(([id, slot]) => ({ id, ...slot }));
  }

  function normalizeHomeSlot(slot) {
    const w = Math.min(HOME_COLS, Math.max(HOME_MIN_W, Number(slot.w) || 5));
    const h = Math.min(60, Math.max(HOME_MIN_H, Number(slot.h) || 8));
    const x = Math.min(HOME_COLS - w, Math.max(0, Number(slot.x) || 0));
    const y = Math.min(200, Math.max(0, Number(slot.y) || 0));
    return { id: slot.id, x, y, w, h };
  }

  function readHomeLayout() {
    const raw = Array.isArray(state?.home_layout) ? state.home_layout : [];
    const found = new Map();
    for (const item of raw) {
      if (item && typeof item === 'object' && HOME_DEFAULTS[item.id]) {
        found.set(item.id, normalizeHomeSlot(item));
      }
    }
    return Object.keys(HOME_DEFAULTS).map(
      (id) => found.get(id) || { id, ...HOME_DEFAULTS[id] }
    );
  }

  function homeMetrics() {
    const grid = $('#homeGrid');
    const width = grid ? grid.clientWidth : 0;
    const col = (width - HOME_GAP * (HOME_COLS - 1)) / HOME_COLS;
    return { col, stepX: col + HOME_GAP, stepY: HOME_ROW };
  }

  function ensureResizeHandles() {
    homeWidgets().forEach((widget) => {
      if (widget.querySelector('.widget-resize')) return;
      const handle = document.createElement('button');
      handle.type = 'button';
      handle.className = 'widget-resize';
      handle.setAttribute('aria-label', isEnglish() ? 'Resize' : 'Redimensionar');
      widget.appendChild(handle);
    });
  }

  function applyHomeGrid() {
    const grid = $('#homeGrid');
    if (!grid || !grid.clientWidth) return;
    if (window.matchMedia('(max-width: 900px)').matches) {
      grid.style.height = '';
      homeWidgets().forEach((widget) => {
        widget.style.left = '';
        widget.style.top = '';
        widget.style.width = '';
        widget.style.height = '';
      });
      return;
    }
    const { col } = homeMetrics();
    let rows = 0;
    for (const slot of homeLayout) {
      const widget = grid.querySelector(`[data-widget="${slot.id}"]`);
      if (!widget) continue;
      widget.style.left = `${slot.x * (col + HOME_GAP)}px`;
      widget.style.top = `${slot.y * HOME_ROW}px`;
      widget.style.width = `${slot.w * col + (slot.w - 1) * HOME_GAP}px`;
      widget.style.height = `${slot.h * HOME_ROW - HOME_GAP}px`;
      rows = Math.max(rows, slot.y + slot.h);
    }
    grid.style.height = `${rows * HOME_ROW}px`;
  }

  function showHomePlaceholder(slot) {
    let placeholder = $('#homePlaceholder');
    if (!placeholder) {
      placeholder = document.createElement('div');
      placeholder.id = 'homePlaceholder';
      placeholder.className = 'widget-placeholder';
      $('#homeGrid')?.appendChild(placeholder);
    }
    const { col } = homeMetrics();
    placeholder.style.left = `${slot.x * (col + HOME_GAP)}px`;
    placeholder.style.top = `${slot.y * HOME_ROW}px`;
    placeholder.style.width = `${slot.w * col + (slot.w - 1) * HOME_GAP}px`;
    placeholder.style.height = `${slot.h * HOME_ROW - HOME_GAP}px`;
  }

  function hideHomePlaceholder() {
    $('#homePlaceholder')?.remove();
  }

  function homeOverlaps(a, b) {
    return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
  }

  function resolveHomeCollisions(movedId) {
    const moved = homeLayout.find((slot) => slot.id === movedId);
    if (!moved) return;
    const placed = [moved];
    const others = homeLayout
      .filter((slot) => slot.id !== movedId)
      .sort((a, b) => a.y - b.y || a.x - b.x);
    for (const slot of others) {
      let guard = 0;
      while (placed.some((other) => homeOverlaps(slot, other)) && guard < 100) {
        const hit = placed.find((other) => homeOverlaps(slot, other));
        slot.y = hit.y + hit.h;
        guard += 1;
      }
      placed.push(slot);
    }
  }

  async function saveHomeLayout() {
    try {
      await api('/api/home-layout', {
        method: 'POST',
        body: JSON.stringify({ layout: homeLayout }),
      });
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  function renderHomeLayout() {
    if (homeCustomizing) return;
    homeLayout = readHomeLayout();
    ensureResizeHandles();
    applyHomeGrid();
  }

  function setHomeCustomizing(on) {
    const wasOn = homeCustomizing;
    homeCustomizing = on;
    $('#homeGrid')?.classList.toggle('customizing', on);
    $('#resetLayoutBtn')?.classList.toggle('hidden', !on || currentView !== 'send');
    $('#customizeLabel').textContent = on
      ? (isEnglish() ? 'Done' : 'Listo')
      : (isEnglish() ? 'Customize' : 'Personalizar');
    if (on) {
      homeLayout = readHomeLayout();
      ensureResizeHandles();
      applyHomeGrid();
    } else {
      hideHomePlaceholder();
      homeWidgets().forEach((widget) => widget.classList.remove('dragging'));
      applyHomeGrid();
      if (wasOn) saveHomeLayout();
    }
  }

  function finishHomeDrag() {
    if (!homeDrag) return;
    const { id, target, widget } = homeDrag;
    homeDrag = null;
    widget.classList.remove('dragging');
    widget.style.transform = '';
    hideHomePlaceholder();
    const slot = homeLayout.find((item) => item.id === id);
    if (slot) Object.assign(slot, target);
    resolveHomeCollisions(id);
    applyHomeGrid();
    saveHomeLayout();
  }

  function setupHomeCustomize() {
    const grid = $('#homeGrid');
    if (!grid) return;
    grid.addEventListener('pointerdown', (event) => {
      if (!homeCustomizing || event.button !== 0) return;
      const widget = event.target.closest('[data-widget]');
      if (!widget) return;
      const slot = homeLayout.find((item) => item.id === widget.dataset.widget);
      if (!slot) return;
      event.preventDefault();
      homeDrag = {
        id: slot.id,
        resize: !!event.target.closest('.widget-resize'),
        widget,
        startX: event.clientX,
        startY: event.clientY,
        slot: { ...slot },
        target: { ...slot },
      };
      widget.classList.add('dragging');
      grid.setPointerCapture(event.pointerId);
    });
    grid.addEventListener('pointermove', (event) => {
      if (!homeDrag) return;
      const { col, stepX, stepY } = homeMetrics();
      const dx = Math.round((event.clientX - homeDrag.startX) / stepX);
      const dy = Math.round((event.clientY - homeDrag.startY) / stepY);
      if (homeDrag.resize) {
        const w = Math.min(
          HOME_COLS - homeDrag.slot.x,
          Math.max(HOME_MIN_W, homeDrag.slot.w + dx)
        );
        const h = Math.max(HOME_MIN_H, homeDrag.slot.h + dy);
        homeDrag.target = { ...homeDrag.slot, w, h };
        homeDrag.widget.style.width = `${w * col + (w - 1) * HOME_GAP}px`;
        homeDrag.widget.style.height = `${h * HOME_ROW - HOME_GAP}px`;
      } else {
        const x = Math.min(
          HOME_COLS - homeDrag.slot.w,
          Math.max(0, homeDrag.slot.x + dx)
        );
        const y = Math.max(0, homeDrag.slot.y + dy);
        homeDrag.target = { ...homeDrag.slot, x, y };
        homeDrag.widget.style.transform = `translate(${event.clientX - homeDrag.startX}px, ${event.clientY - homeDrag.startY}px)`;
      }
      showHomePlaceholder(homeDrag.target);
    });
    grid.addEventListener('pointerup', finishHomeDrag);
    grid.addEventListener('pointercancel', finishHomeDrag);
  }

  // ---- Release notes ----

  function changelogMediaUrl(url) {
    return typeof url === 'string' && /^https:\/\/www\.peerway\.me\/updates\//.test(url);
  }

  function changelogText(value) {
    if (typeof value === 'string') return value;
    if (value && typeof value === 'object') {
      return value[isEnglish() ? 'en' : 'es'] || value.es || value.en || '';
    }
    return '';
  }

  function showChangelog(data) {
    const body = $('#changelogBody');
    if (!body) return;
    $('#changelogTitle').textContent = changelogText(data.title) || (isEnglish() ? "What's new" : 'Qué cambió');
    body.textContent = '';
    for (const section of Array.isArray(data.sections) ? data.sections : []) {
      const wrap = document.createElement('section');
      wrap.className = 'changelog-section';
      const sectionTitle = changelogText(section.title);
      if (sectionTitle) {
        const title = document.createElement('h3');
        title.textContent = sectionTitle;
        wrap.appendChild(title);
      }
      const sectionBody = changelogText(section.body);
      if (sectionBody) {
        const text = document.createElement('p');
        text.textContent = sectionBody;
        wrap.appendChild(text);
      }
      const media = Array.isArray(section.media) ? section.media : [];
      const box = document.createElement('div');
      box.className = 'changelog-media';
      for (const item of media) {
        if (!item || !changelogMediaUrl(item.src)) continue;
        if (item.type === 'video') {
          const video = document.createElement('video');
          video.src = item.src;
          video.controls = true;
          video.preload = 'metadata';
          if (changelogMediaUrl(item.poster)) video.poster = item.poster;
          box.appendChild(video);
        } else {
          const image = document.createElement('img');
          image.src = item.src;
          image.alt = item.alt || '';
          image.loading = 'lazy';
          box.appendChild(image);
        }
      }
      if (box.children.length) wrap.appendChild(box);
      body.appendChild(wrap);
    }
    if (!body.children.length) return;
    $('#changelogOverlay').classList.remove('hidden');
    $('#changelogDialog').focus();
  }

  async function markChangelogSeen(version, disable) {
    try {
      await api('/api/changelog/seen', {
        method: 'POST',
        body: JSON.stringify({ version, disable }),
      });
      if (state) {
        state.last_seen_version = version;
        if (disable) state.changelog_disabled = true;
      }
    } catch {
      // The dialog is already closed; try again on the next start.
    }
  }

  async function maybeShowChangelog(s) {
    if (!s?.version || s.changelog_disabled) return;
    if (s.last_seen_version === s.version) return;
    if (!s.last_seen_version) {
      // Fresh install: remember this version without showing old notes.
      await markChangelogSeen(s.version, false);
      return;
    }
    changelogShown = true;
    try {
      const data = await api('/api/changelog');
      if (data?.version === s.version) showChangelog(data);
    } catch {
      // Offline or notes not published yet: retry on the next start.
    }
  }

  async function dismissChangelog() {
    const disable = !!$('#changelogDisable')?.checked;
    $('#changelogOverlay')?.classList.add('hidden');
    if (state?.version) await markChangelogSeen(state.version, disable);
  }

  // State pushes arrive while the user types; never overwrite an input that is
  // focused or holds unsaved edits.
  function syncEditable(input, value) {
    if (!input || document.activeElement === input || input.dataset.dirty === '1') return;
    if (input.value !== value) input.value = value;
  }

  function renderCloudAccount(s) {    const logged = !!s.logged_in;
    $('#cloudLoggedOut').classList.toggle('hidden', logged);
    $('#cloudLoggedIn').classList.toggle('hidden', !logged);
    if (!logged) return;
    $('#cloudUsername').textContent = s.display_name || s.username || 'Cuenta';
    const displayInput = $('#cloudDisplayName');
    if (displayInput) {
      syncEditable(displayInput, s.display_name || '');
      displayInput.placeholder = s.username || 'Tu nombre';
    }
    const emailInput = $('#cloudEmail');
    if (emailInput) {
      syncEditable(emailInput, s.email || '');
      emailInput.placeholder = 'tu@correo.com';
    }
    const emailState = $('#cloudEmailState');
    if (emailState) {
      emailState.textContent = !s.email
        ? (isEnglish() ? 'No email yet. Add one to use the cloud.' : 'Sin correo. Añade uno para usar la nube.')
        : s.email_verified
          ? (isEnglish() ? 'Email confirmed.' : 'Correo confirmado.')
          : (isEnglish() ? 'Email not confirmed yet. Check your inbox.' : 'Correo sin confirmar. Revisa tu bandeja.');
    }
    const emailNotice = $('#cloudEmailNotice');
    if (emailNotice) emailNotice.classList.toggle('hidden', s.email_verified !== false);
    $('#cloudAccountSub').textContent = `${s.plan_label || 'Gratis'} · código ${s.code}`;
    const isPro = s.plan === 'pro' || s.plan === 'team';
    updateCloudPlanPanels();
    $('#cloudStorage').classList.remove('hidden');
    const quota = cloudData?.quota_bytes || s.quota_bytes || 0;
    const used = cloudData?.used_bytes ?? s.used_bytes ?? 0;
    const percent = quota > 0 ? Math.min(100, (used / quota) * 100) : 0;
    $('#quotaFill').style.width = `${percent.toFixed(1)}%`;
    $('#quotaText').textContent = `${formatBytes(used)} de ${formatBytes(quota)}`;
    $('#quotaPercent').textContent = `${percent.toFixed(percent < 10 ? 1 : 0)}%`;
    const retention = Number(cloudData?.retention_days ?? (isPro ? 0 : (plans?.free?.retention_days ?? 30)));
    $('#cloudRetentionNote').textContent = retention > 0
      ? (isEnglish()
        ? `Each Free-plan file is deleted automatically ${retention} days after upload.`
        : `Cada archivo del plan Gratis se elimina automáticamente ${retention} días después de la subida.`)
      : (isEnglish()
        ? 'Your files are kept while your plan is active.'
        : 'Tus archivos se conservan mientras el plan esté activo.');
  }

  async function saveDisplayName() {
    const input = $('#cloudDisplayName');
    if (!input) return;
    const displayName = input.value.trim();
    try {
      await api('/api/account/display-name', {
        method: 'POST',
        body: JSON.stringify({ display_name: displayName }),
      });
      toast(isEnglish() ? 'Display name saved.' : 'Nombre visible guardado.', 'success');
      input.dataset.dirty = '0';
      await loadState();
      await loadCloud();
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  function maybeShowOffer() {
    if (activeOffer || !state) return;
    const next = (state.transfers || []).find((t) => t.status === 'offered');
    if (!next) return;
    activeOffer = next;
    const files = next.files || [];
    $('#offerEyebrow').textContent = next.direction === 'receive' ? 'SOLICITUD ENTRANTE' : 'TRANSFERENCIA';
    $('#offerTitle').textContent = `${next.peer_name || 'Un equipo'} quiere enviarte archivos`;
    $('#offerSubtitle').textContent = `${next.peer_code} · ${files.length} archivo${files.length === 1 ? '' : 's'}${next.comment ? ` · ${next.comment}` : ''}`;
    $('#offerSize').textContent = formatBytes(next.total_bytes);
    $('#offerChannel').textContent = next.channel || 'Canal cifrado';
    $('#offerOverlay').classList.remove('hidden');
  }

  function hideOffer() {
    activeOffer = null;
    $('#offerOverlay').classList.add('hidden');
  }

  async function decideOffer(accept) {
    if (!activeOffer) return;
    const id = activeOffer.id;
    try {
      await api(`/api/transfers/${id}/${accept ? 'accept' : 'decline'}`, { method: 'POST' });
      toast(accept ? 'Recepción aceptada.' : 'Solicitud rechazada.', accept ? 'success' : 'info');
    } catch (error) {
      toast(error.message, 'error');
    }
    hideOffer();
    await loadState();
    setTimeout(maybeShowOffer, 250);
  }

  function addFiles(entries) {
    const dedupe = new Set(selected.map((f) => f.path));
    for (const entry of entries) {
      if (!entry?.path || dedupe.has(entry.path)) continue;
      dedupe.add(entry.path);
      selected.push(entry);
    }
    renderSelected();
  }

  function renderSelected() {
    const box = $('#selectedFiles');
    box.classList.toggle('hidden', selected.length === 0);
    const total = selected.reduce((sum, f) => sum + (Number(f.size) || 0), 0);
    $('#selectedSummary').textContent = `${selected.length} archivo${selected.length === 1 ? '' : 's'} · ${formatBytes(total)}`;
    $('#fileList').innerHTML = selected.map((file, index) => `
      <div class="file-row">
        <div class="file-icon">${icon('files')}</div>
        <div class="file-info"><strong>${escapeHtml(file.name)}</strong><span>${formatBytes(file.size)}</span></div>
        <button class="icon-btn" data-remove-file="${index}" title="Quitar" aria-label="Quitar">${icon('x')}</button>
      </div>`).join('');
    $('#sendBtn').disabled = !canSend();
    const shareButton = $('#shareWebBtn');
    if (shareButton) shareButton.disabled = selected.length === 0;
    const comment = $('#sendComment');
    if (comment) {
      comment.disabled = selected.length === 0;
      if (selected.length === 0) comment.value = '';
    }
  }

  function canSend() {
    const pending = normalizeCode($('#targetCode')?.value || '');
    return selected.length > 0 && (recipients.length > 0 || pending.length === 8);
  }

  function addRecipient(value, name) {
    const code = normalizeCode(value);
    if (code.length !== 8) return false;
    if (!recipients.some((item) => item.code === code)) {
      recipients.push({ code, name: name || '' });
    }
    renderRecipients();
    return true;
  }

  function removeRecipient(code) {
    recipients = recipients.filter((item) => item.code !== code);
    renderRecipients();
  }

  function renderRecipients() {
    const box = $('#recipientChips');
    if (!box) return;
    box.classList.toggle('hidden', recipients.length === 0);
    box.innerHTML = recipients.map((item) => `
      <span class="recipient-chip">${escapeHtml(formatCode(item.code))}
        <button class="icon-btn" data-remove-recipient="${item.code}" title="Quitar" aria-label="Quitar">${icon('x')}</button>
      </span>`).join('');
    $('#sendBtn').disabled = !canSend();
  }

  function loadRecentRecipients() {
    try {
      const stored = JSON.parse(localStorage.getItem(RECENT_KEY) || '[]');
      recentRecipients = Array.isArray(stored) ? stored : [];
    } catch {
      recentRecipients = [];
    }
    renderRecentRecipients();
  }

  function rememberRecipients(items) {
    for (const item of items) {
      recentRecipients = recentRecipients.filter((entry) => entry.code !== item.code);
      recentRecipients.unshift({
        code: item.code,
        name: item.name || '',
        at: Math.floor(Date.now() / 1000),
      });
    }
    recentRecipients = recentRecipients.slice(0, 8);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recentRecipients));
    } catch {}
    renderRecentRecipients();
  }

  function renderRecentRecipients() {
    const list = $('#recentRecipients');
    if (!list) return;
    list.innerHTML = recentRecipients.map((item) => `
      <button class="recent-item" data-recent-code="${item.code}" type="button">
        <strong>${escapeHtml(formatCode(item.code))}</strong>
        <span>${escapeHtml(item.name || '')}</span>
      </button>`).join('');
    $('#recentEmpty')?.classList.toggle('hidden', recentRecipients.length > 0);
  }

  function pickFiles() {
    // A normal file input is more reliable than launching a native dialog from
    // the background service, especially on Windows. The files are staged in
    // the local area before sending, exactly like a drag and drop.
    $('#localFileInput').click();
  }

  async function stageFiles(files) {
    if (!files.length) return;
    const form = new FormData();
    for (const file of files) form.append('file', file, file.name);
    toast('Copiando archivos al área local…', 'info');
    try {
      const result = await api('/api/stage', { method: 'POST', body: form });
      addFiles(result.files || []);
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  async function sendSelected() {
    // Typing a code and pressing Send still works: it becomes a recipient.
    const pending = normalizeCode($('#targetCode').value);
    if (pending.length === 8) {
      addRecipient(pending);
      $('#targetCode').value = '';
      $('#codeOk').classList.add('hidden');
    }
    if (!recipients.length) {
      toast('Añade al menos un destinatario.', 'warning');
      return;
    }
    if (!selected.length) return;
    $('#sendBtn').disabled = true;
    const comment = ($('#sendComment')?.value || '').trim();
    const paths = selected.map((file) => file.path);
    const targets = recipients.map((item) => item.code);
    try {
      // One independent transfer per recipient: the sender works as the hub.
      const results = await Promise.allSettled(
        targets.map((code) => api('/api/send', {
          method: 'POST',
          body: JSON.stringify({ code, paths, comment }),
        }))
      );
      const failed = results.filter((result) => result.status === 'rejected');
      const sent = targets.length - failed.length;
      if (sent === 0) {
        throw failed[0].reason || new Error('No se pudo iniciar el envío.');
      }
      toast(
        sent === targets.length
          ? `Transferencia iniciada para ${sent} destinatario${sent === 1 ? '' : 's'}.`
          : `Enviado a ${sent} de ${targets.length}; ${failed[0].reason?.message || 'alguno falló'}.`,
        failed.length ? 'warning' : 'success'
      );
      selected = [];
      rememberRecipients(recipients.map((item) => ({ code: item.code, name: item.name })));
      recipients = [];
      renderSelected();
      renderRecipients();
      $('#targetCode').value = '';
      $('#codeOk').classList.add('hidden');
      const commentInput = $('#sendComment');
      if (commentInput) commentInput.value = '';
      await loadState();
    } catch (error) {
      toast(error.message, 'error');
    } finally {
      $('#sendBtn').disabled = !canSend();
    }
  }

  async function loadCloud() {
    if (!state?.logged_in) return;
    try {
      cloudData = await api('/api/cloud');
      renderCloudFiles();
      renderCloudAccount(state);
    } catch (error) {
      if (/sesión|inicia sesión/i.test(error.message)) {
        toast('Inicia sesión de nuevo para usar la nube.', 'warning');
      } else {
        toast(error.message, 'error');
      }
    }
  }

  function paidPlansAvailable(candidate = plans) {
    if (!candidate || typeof candidate !== 'object') return false;
    // Older servers have no flag. Require both an explicit opt-in and PayPal;
    // anything else keeps paid offers hidden.
    if (candidate.paid_billing_enabled === false) return false;
    if (candidate.paypal_enabled !== true) return false;
    return candidate.paid_billing_enabled === true || candidate.paid_plans_enabled === true;
  }

  function updateCloudPlanPanels() {
    const logged = !!state?.logged_in;
    const isPro = state?.plan === 'pro' || state?.plan === 'team';
    const freeUser = logged && !isPro;
    const paid = paidPlansAvailable();
    $('#cloudFree')?.classList.toggle('hidden', !freeUser);
    $('#cloudPaidOffer')?.classList.toggle('hidden', !(freeUser && paid));
  }

  async function loadPlans() {
    try {
      plans = await api('/api/plans');
      applyPlans();
    } catch {
      // Keep conservative built-in defaults when the server is offline: the
      // static free-beta panel stays and paid offers stay hidden.
      plans = null;
      updateCloudPlanPanels();
      hidePayPal();
    }
  }

  function applyPlans() {
    if (!plans) return;
    const free = plans.free || { quota_bytes: 5 * 1024 * 1024 * 1024, retention_days: 30 };
    const freeQuota = formatBytes(free.quota_bytes || 0);
    const freeRetention = Number(free.retention_days || 30);
    const english = isEnglish();
    const paid = paidPlansAvailable() && plans.pro && plans.team;
    const quota = paid ? formatBytes(plans.pro.quota_bytes || 0) : '';
    $('#cloudFreeQuotaCopy').textContent = freeQuota;
    $('#cloudFreeQuotaPoint').textContent = freeQuota;
    $('#cloudFreeQuotaPoint2').textContent = freeQuota;
    $('#cloudFreeRetentionCopy').textContent = english ? `${freeRetention} days` : `${freeRetention} días`;
    $('#cloudFreeRetentionPoint').textContent = english ? `${freeRetention} days` : `${freeRetention} días`;
    if (paid) {
      $('#cloudPrice').textContent = `$${plans.pro.price_usd}`;
      $('#upgradePrice').textContent = `$${plans.pro.price_usd}`;
      $('#cloudFeatureQuota').textContent = english
        ? `${freeQuota} free · ${quota} Pro`
        : `${freeQuota} gratis · ${quota} Pro`;
      $('#cloudProQuotaCopy').textContent = quota;
      $('#cloudFreeNote').textContent = english
        ? `Free: ${freeQuota} in total; each file is deleted ${freeRetention} days after upload.`
        : `Gratis: ${freeQuota} en total; cada archivo se elimina ${freeRetention} días después de la subida.`;
      if (!plans.cloud_enabled) {
        $('#cloudPriceNote').textContent = english ? 'Cloud disabled' : 'Nube desactivada';
        $('#activateBtn').disabled = true;
      } else {
        $('#cloudPriceNote').textContent = `Pro · ${quota}`;
        $('#activateBtn').disabled = false;
      }
    } else {
      $('#cloudPrice').textContent = english ? 'Free' : 'Gratis';
      $('#cloudFeatureQuota').textContent = english
        ? `${freeQuota} free · deleted after ${freeRetention} days`
        : `${freeQuota} gratis · borrado a los ${freeRetention} días`;
      $('#cloudFreeNote').textContent = english
        ? `Free beta: ${freeQuota} in total; each file is deleted ${freeRetention} days after upload.`
        : `Beta gratuita: ${freeQuota} en total; cada archivo se elimina ${freeRetention} días después de la subida.`;
      $('#cloudPriceNote').textContent = english
        ? `Free beta · ${freeQuota} in total · deleted after ${freeRetention} days`
        : `Beta gratuita · ${freeQuota} en total · borrado a los ${freeRetention} días`;
    }
    updateCloudPlanPanels();
    if (paid) setupPayPal();
    else hidePayPal();
  }

  function renderCloudFiles() {
    if (!cloudData) return;
    const files = cloudData.files || [];
    const retention = Number(cloudData.retention_days ?? (cloudData.plan === 'free' ? (plans?.free?.retention_days ?? 30) : 0));
    $('#cloudFiles').innerHTML = files.map((file) => {
      const expiry = retention > 0 ? formatDate(file.created_at + retention * 86400) : '';
      const meta = expiry
        ? `${formatBytes(file.size)} · ${formatDate(file.created_at)} · ${isEnglish() ? 'deletes' : 'se elimina'} ${expiry}`
        : `${formatBytes(file.size)} · ${formatDate(file.created_at)}`;
      return `
      <div class="cloud-row">
        <div class="file-icon">${icon('files')}</div>
        <div class="cloud-main"><strong>${escapeHtml(file.name)}</strong><span>${meta}</span></div>
        <div class="cloud-actions">
          <button class="icon-btn" data-download="${escapeHtml(file.id)}" title="Descargar" aria-label="Descargar">${icon('download')}</button>
          <button class="icon-btn" data-cloud-delete="${escapeHtml(file.id)}" title="Borrar" aria-label="Borrar">${icon('trash')}</button>
        </div>
      </div>`;
    }).join('');
    $('#cloudEmpty').classList.toggle('hidden', files.length > 0);
  }

  async function uploadCloudFile(file) {
    if (!file) return;
    const errorBox = $('#cloudUploadError');
    const showError = (message) => {
      if (!errorBox) return;
      errorBox.textContent = message;
      errorBox.classList.remove('hidden');
    };
    const clearError = () => {
      if (!errorBox) return;
      errorBox.textContent = '';
      errorBox.classList.add('hidden');
    };
    clearError();
    if (state && state.quota_bytes > 0) {
      const remaining = Math.max(0, state.quota_bytes - (state.used_bytes || 0));
      if (file.size > remaining) {
        const message = isEnglish()
          ? `The file is larger than the remaining space (${formatBytes(remaining)}).`
          : `El archivo supera el espacio disponible (${formatBytes(remaining)}).`;
        showError(message);
        toast(message, 'error');
        return;
      }
    }
    const form = new FormData();
    form.append('file', file, file.name);
    const button = $('#cloudUploadBtn');
    const reset = () => {
      uploading = false;
      button.disabled = false;
      button.innerHTML = `${icon('plus')} ${isEnglish() ? 'Upload file' : 'Subir archivo'}`;
    };
    uploading = true;
    button.disabled = true;
    button.textContent = isEnglish() ? 'Uploading…' : 'Subiendo…';
    const xhr = new XMLHttpRequest();
    xhr.open('POST', '/api/cloud/upload');
    // No timeout: a large file over a slow connection takes much longer than
    // the old 10 minutes, and the node reports real progress meanwhile.
    xhr.timeout = 0;
    xhr.setRequestHeader('x-peerway-token', TOKEN);
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) {
        const percent = Math.round((event.loaded / event.total) * 100);
        button.textContent = isEnglish() ? `Uploading ${percent}%` : `Subiendo ${percent}%`;
      }
    };
    xhr.upload.onload = () => {
      button.textContent = isEnglish() ? 'Saving to the cloud…' : 'Guardando en la nube…';
    };
    xhr.onload = async () => {
      reset();
      if (xhr.status >= 200 && xhr.status < 300) {
        clearError();
        toast(isEnglish() ? `“${file.name}” saved to the cloud.` : `«${file.name}» guardado en la nube.`, 'success');
        await loadCloud();
        await loadState();
      } else {
        let message = isEnglish() ? 'The file could not be uploaded.' : 'No se pudo subir el archivo.';
        try { message = JSON.parse(xhr.responseText).error || message; } catch {}
        showError(message);
        toast(message, 'error');
      }
    };
    xhr.onerror = () => {
      reset();
      const message = isEnglish() ? 'Network error while uploading. The file was not saved.' : 'Error de red al subir el archivo. No se guardó.';
      showError(message);
      toast(message, 'error');
    };
    xhr.ontimeout = () => {
      reset();
      const message = isEnglish() ? 'The upload timed out. Try again.' : 'La subida tardó demasiado. Inténtalo de nuevo.';
      showError(message);
      toast(message, 'error');
    };
    xhr.onabort = reset;
    xhr.send(form);
  }

  async function downloadCloud(id) {
    const file = (cloudData?.files || []).find((f) => f.id === id);
    const link = document.createElement('a');
    link.href = `/api/cloud/${encodeURIComponent(id)}/raw?token=${encodeURIComponent(TOKEN)}`;
    link.download = file?.name || 'archivo-peerway';
    document.body.appendChild(link);
    link.click();
    link.remove();
  }

  async function deleteCloud(id) {
    const file = (cloudData?.files || []).find((f) => f.id === id);
    const question = `¿Borrar «${file?.name || 'archivo'}» de la nube?`;
    if (!confirm(window.PeerWayI18n?.t?.(question) || question)) return;
    try {
      await api(`/api/cloud/${id}`, { method: 'DELETE' });
      toast('Archivo borrado de la nube.', 'info');
      await loadCloud();
      await loadState();
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  function showAuthError(message) {
    const box = $('#authError');
    if (!box) return;
    box.textContent = message || '';
    box.classList.toggle('hidden', !message);
  }

  async function authenticate(event) {
    event.preventDefault();
    const username = $('#authUser').value.trim();
    const password = $('#authPass').value;
    const email = $('#authEmail')?.value.trim() || '';
    const english = isEnglish();
    // Mirror the server rules here so an obvious mistake does not burn the
    // per-IP registration quota.
    if (username.length < 3 || username.length > 32 || !/^[A-Za-z0-9_.-]+$/.test(username)) {
      showAuthError(english
        ? 'Username: 3-32 characters, no spaces or accents (letters, numbers, dot, dash, underscore).'
        : 'Usuario: de 3 a 32 caracteres, sin espacios ni acentos (letras, números, punto, guion y guion bajo).');
      return;
    }
    if (password.length < 8) {
      showAuthError(english
        ? 'Password: at least 8 characters.'
        : 'Contraseña: al menos 8 caracteres.');
      return;
    }
    if (authMode === 'register' && !email) {
      showAuthError(english
        ? 'Write your email to confirm the account.'
        : 'Escribe tu correo para confirmar la cuenta.');
      return;
    }
    if (authMode === 'register' && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) {
      showAuthError(english
        ? 'That email does not look valid.'
        : 'Ese correo no parece válido.');
      return;
    }
    showAuthError('');
    const button = $('#authSubmit');
    button.disabled = true;
    button.textContent = authMode === 'register' ? 'Creando cuenta…' : 'Entrando…';
    try {
      await api(`/api/auth/${authMode}`, {
        method: 'POST',
        body: JSON.stringify({ username, password, email }),
      });
      toast(authMode === 'register'
        ? 'Cuenta creada. Revisa tu correo para confirmarlo.'
        : 'Sesión iniciada.', 'success');
      $('#authPass').value = '';
      await loadState();
      await loadCloud();
    } catch (error) {
      showAuthError(error.message);
    } finally {
      button.disabled = false;
      button.textContent = authMode === 'register' ? 'Crear cuenta' : 'Iniciar sesión';
    }
  }

  async function activateLicense() {
    if (!paidPlansAvailable()) {
      toast('Los planes de pago no están disponibles durante la beta gratuita.', 'warning');
      return;
    }
    const key = $('#licenseKey').value.trim();
    if (!key) {
      toast('Escribe una licencia.', 'warning');
      return;
    }
    $('#activateBtn').disabled = true;
    try {
      await api('/api/cloud/activate', { method: 'POST', body: JSON.stringify({ license_key: key }) });
      toast('Plan Pro activado.', 'success');
      $('#licenseKey').value = '';
      await loadState();
      await loadCloud();
    } catch (error) {
      toast(error.message, 'error');
    } finally {
      $('#activateBtn').disabled = false;
    }
  }

  async function checkout() {
    if (!paidPlansAvailable()) {
      toast('Los planes de pago no están disponibles durante la beta gratuita.', 'warning');
      return;
    }
    try {
      const result = await api('/api/cloud/checkout', { method: 'POST', body: '{}' });
      if (result.url) {
        await api('/api/open-url', { method: 'POST', body: JSON.stringify({ url: result.url }) });
      } else {
        toast('No hay una URL de pago configurada.', 'warning');
      }
    } catch (error) {
      toast(error.message, 'warning');
    }
  }

  let paypalLoading = false;
  let paypalRendered = false;

  function hidePayPal() {
    $('#paypalButtons')?.classList.add('hidden');
    const note = $('#paypalNote');
    if (note) {
      note.textContent = isEnglish()
        ? 'Paid plans are not available during the free beta.'
        : 'Los planes de pago no están disponibles durante la beta gratuita.';
    }
  }

  function setupPayPal() {
    const container = $('#paypalButtons');
    const note = $('#paypalNote');
    if (!container) return;
    if (!paidPlansAvailable() || !plans?.paypal_enabled || !plans.paypal_client_id) {
      hidePayPal();
      return;
    }
    if (window.paypal) {
      renderPayPalButtons();
      return;
    }
    if (paypalLoading) return;
    paypalLoading = true;
    const existing = document.querySelector('script[data-peerway-paypal]');
    if (existing) return;
    const script = document.createElement('script');
    script.dataset.peerwayPaypal = '1';
    script.src = `https://www.paypal.com/sdk/js?client-id=${encodeURIComponent(plans.paypal_client_id)}&currency=${encodeURIComponent(plans.paypal_currency || 'USD')}&intent=capture`;
    script.onload = () => {
      paypalLoading = false;
      renderPayPalButtons();
    };
    script.onerror = () => {
      paypalLoading = false;
      if (note) note.textContent = isEnglish()
        ? 'PayPal could not load. Check your connection.'
        : 'No se pudo cargar PayPal. Revisa tu conexión.';
    };
    document.head.appendChild(script);
  }

  function renderPayPalButtons() {
    const container = $('#paypalButtons');
    const note = $('#paypalNote');
    if (!container || !window.paypal || paypalRendered) return;
    paypalRendered = true;
    container.classList.remove('hidden');
    if (note) note.textContent = isEnglish()
      ? 'Pay with PayPal. One-time payment for 30 days; no automatic renewal.'
      : 'Pago con PayPal. Pago único por 30 días; sin renovación automática.';
    window.paypal.Buttons({
      style: { layout: 'vertical', color: 'black', shape: 'pill', label: 'paypal', height: 40 },
      createOrder: async () => {
        const order = await api('/api/cloud/paypal/order', {
          method: 'POST',
          body: JSON.stringify({ plan: 'pro' }),
        });
        return order.order_id;
      },
      onApprove: async (data) => {
        try {
          await api('/api/cloud/paypal/capture', {
            method: 'POST',
            body: JSON.stringify({ order_id: data.orderID }),
          });
          toast('Pago completado. Plan Pro activado.', 'success');
          await loadState();
          await loadCloud();
        } catch (error) {
          toast(error.message, 'error');
        }
      },
      onError: (error) => {
        console.error('PayPal', error);
        toast('No se pudo completar el pago con PayPal.', 'error');
      },
    }).render('#paypalButtons');
  }

  async function saveSettings() {
    const config = {
      device_name: $('#setDevice').value.trim(),
      download_dir: $('#setDownload').value.trim(),
      auto_accept_trusted: $('#setAutoTrust').checked,
      lan_discovery: $('#setLan').checked,
      auto_update: $('#setAutoUpdate').checked,
      changelog_disabled: !$('#setChangelog').checked,
      autostart_enabled: $('#setAutostart').checked,
      telemetry_enabled: $('#setTelemetry').checked,
      telemetry_choice_made: true,
      notifications_enabled: $('#setNotifications').checked,
      notify_updates: $('#setNotifyUpdates').checked,
      notify_transfers: $('#setNotifyTransfers').checked,
      language: window.PeerWayI18n?.lang || state?.language || 'es',
    };
    try {
      await api('/api/settings', { method: 'POST', body: JSON.stringify(config) });
      settingsDirty = false;
      toast('Ajustes guardados.', 'success');
      await loadState();
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  async function addTrusted() {
    const code = normalizeCode($('#trustCode').value);
    if (code.length !== 8) {
      toast('Código inválido.', 'warning');
      return;
    }
    try {
      await api('/api/trust', { method: 'POST', body: JSON.stringify({ code, trusted: true }) });
      $('#trustCode').value = '';
      toast('Contacto agregado como confiable.', 'success');
      await loadState();
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  async function removeTrusted(code) {
    try {
      await api('/api/trust', { method: 'POST', body: JSON.stringify({ code, trusted: false }) });
      await loadState();
    } catch (error) {
      toast(error.message, 'error');
    }
  }

  function connectEvents() {
    const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/events?token=${encodeURIComponent(TOKEN)}`;
    try {
      ws = new WebSocket(url);
    } catch {
      setTimeout(connectEvents, wsRetry);
      return;
    }
    ws.onopen = () => { wsRetry = 1000; };
    ws.onmessage = (event) => {
      let data;
      try { data = JSON.parse(event.data); } catch { return; }
      if (data.type === 'transfer' && data.transfer) {
        if (!state) return;
        const list = state.transfers || [];
        const index = list.findIndex((t) => t.id === data.transfer.id);
        if (index >= 0) list[index] = data.transfer;
        else list.unshift(data.transfer);
        state.transfers = list;
        renderActive(state);
        renderPending(state);
        renderHistory(state);
        renderCloudAccount(state);
        maybeShowOffer();
        return;
      }
      if (data.type === 'incoming' && data.transfer) {
        if (!state) return;
        const list = state.transfers || [];
        if (!list.some((t) => t.id === data.transfer.id)) list.unshift(data.transfer);
        state.transfers = list;
        renderPending(state);
        maybeShowOffer();
        if (state.notifications_enabled !== false && state.notify_transfers !== false) {
          const sender = data.transfer.peer_name || (isEnglish() ? 'A device' : 'Un equipo');
          const count = (data.transfer.files || []).length;
          nativeNotify(
            isEnglish() ? 'Incoming files' : 'Archivos entrantes',
            isEnglish()
              ? `${sender} wants to send you ${count} file${count === 1 ? '' : 's'}.`
              : `${sender} quiere enviarte ${count} archivo${count === 1 ? '' : 's'}.`
          );
        }
        return;
      }
      if (data.type === 'code' && data.formatted) {
        currentCode = data.formatted;
        renderCode();
        return;
      }
      if (data.type === 'cloud_upload') {
        if (!uploading) return;
        const total = Number(data.total) || 0;
        const loaded = Number(data.loaded) || 0;
        const button = $('#cloudUploadBtn');
        if (!button || !total) return;
        const percent = Math.min(100, Math.round((loaded / total) * 100));
        button.textContent = isEnglish()
          ? `Saving to the cloud… ${percent}%`
          : `Guardando en la nube… ${percent}%`;
        return;
      }
      if (data.type === 'toast' && data.message) {
        toast(data.message, data.level || 'info');
        return;
      }
      if (data.type === 'changed' || data.type === 'hello') {
        loadState();
      }
    };
    ws.onclose = () => {
      ws = null;
      const delay = wsRetry;
      wsRetry = Math.min(wsRetry * 1.7, 12000);
      setTimeout(connectEvents, delay);
    };
    ws.onerror = () => { try { ws.close(); } catch {} };
  }

  // Legal and marketing links open in the system browser, never inside the app.
  document.addEventListener('click', (event) => {
    const link = event.target.closest('a[data-external]');
    if (!link) return;
    event.preventDefault();
    const url = link.getAttribute('href');
    api('/api/open-url', { method: 'POST', body: JSON.stringify({ url }) })
      .catch((error) => toast(error.message, 'warning'));
  });

  // Discovery broadcasts stop while the window is hidden (battery on laptops).
  function reportVisibility() {
    api('/api/visibility', {
      method: 'POST',
      body: JSON.stringify({ visible: document.visibilityState === 'visible' }),
    }).catch(() => {});
  }

  function bindEvents() {
    $$('.nav-item').forEach((button) => button.addEventListener('click', () => setView(button.dataset.view)));
  $('#customizeBtn')?.addEventListener('click', () => setHomeCustomizing(!homeCustomizing));
  $('#resetLayoutBtn')?.addEventListener('click', () => {
    homeLayout = defaultHomeLayout();
    applyHomeGrid();
    saveHomeLayout();
  });
  $('#changelogClose')?.addEventListener('click', dismissChangelog);
  $('#changelogDismiss')?.addEventListener('click', dismissChangelog);
  setupHomeCustomize();
    document.addEventListener('click', async (event) => {
      const goto = event.target.closest('[data-goto]');
      if (goto) {
        setView(goto.dataset.goto);
        return;
      }
      const removeFile = event.target.closest('[data-remove-file]');
      if (removeFile) {
        selected.splice(Number(removeFile.dataset.removeFile), 1);
        renderSelected();
        return;
      }
      const accept = event.target.closest('[data-accept]');
      if (accept) {
        activeOffer = state.transfers.find((t) => t.id === accept.dataset.accept) || null;
        $('#offerOverlay').classList.remove('hidden');
        $('#offerModal')?.focus();
        maybeShowOffer();
        await decideOffer(true);
        return;
      }
      const decline = event.target.closest('[data-decline]');
      if (decline) {
        activeOffer = state.transfers.find((t) => t.id === decline.dataset.decline) || null;
        await decideOffer(false);
        return;
      }
      const cancel = event.target.closest('[data-cancel]');
      if (cancel) {
        try {
          await api(`/api/transfers/${cancel.dataset.cancel}/cancel`, { method: 'POST' });
          toast('Transferencia cancelada.', 'info');
          await loadState();
        } catch (error) { toast(error.message, 'error'); }
        return;
      }
      const openFile = event.target.closest('[data-open-file]');
      if (openFile) {
        try { await api(`/api/transfers/${openFile.dataset.openFile}/open-file`, { method: 'POST' }); }
        catch (error) { toast(error.message, 'error'); }
        return;
      }
      const openTransfer = event.target.closest('[data-open-transfer]');
      if (openTransfer) {
        try { await api(`/api/transfers/${openTransfer.dataset.openTransfer}/open`, { method: 'POST' }); }
        catch (error) { toast(error.message, 'error'); }
        return;
      }
      const recent = event.target.closest('[data-recent-code]');
      if (recent) {
        addRecipient(recent.dataset.recentCode);
        setView('send');
        return;
      }
      const removeRecipientButton = event.target.closest('[data-remove-recipient]');
      if (removeRecipientButton) {
        removeRecipient(removeRecipientButton.dataset.removeRecipient);
        return;
      }
      const sendCode = event.target.closest('[data-send-code]');
      if (sendCode) {
        addRecipient(sendCode.dataset.sendCode, sendCode.dataset.sendName);
        setView('send');
        renderSelected();
        return;
      }
      const trustTransfer = event.target.closest('[data-trust-transfer]');
      if (trustTransfer) {
        try {
          await api(`/api/transfers/${trustTransfer.dataset.trustTransfer}/trust`, {
            method: 'POST',
            body: JSON.stringify({ trusted: true }),
          });
          toast(isEnglish() ? 'Device added to trusted contacts.' : 'Equipo agregado a contactos de confianza.', 'success');
          await loadState();
        } catch (error) {
          toast(error.message, 'error');
        }
        return;
      }
      const copyTrusted = event.target.closest('[data-copy-trusted]');
      if (copyTrusted) {
        if (await copyText(copyTrusted.dataset.copyTrusted)) {
          toast(isEnglish() ? 'Code copied.' : 'Código copiado.', 'success');
        }
        return;
      }
      const untrust = event.target.closest('[data-untrust]');
      if (untrust) {
        await removeTrusted(untrust.dataset.untrust);
        return;
      }
      const download = event.target.closest('[data-download]');
      if (download) {
        await downloadCloud(download.dataset.download);
        return;
      }
      const del = event.target.closest('[data-cloud-delete]');
      if (del) {
        await deleteCloud(del.dataset.cloudDelete);
        return;
      }
      const revoke = event.target.closest('[data-revoke-session]');
      if (revoke) {
        await revokeSession(revoke.dataset.revokeSession);
        return;
      }
    });

    $('#accountMini').addEventListener('click', () => setView('cloud'));
    $('#copyCodeBtn').addEventListener('click', async () => {
      if (state && await copyText(state.code)) toast('Código copiado.', 'success');
    });
    $('#copyLinkBtn').addEventListener('click', async () => {
      if (!state?.code) return;
      const code = normalizeCode(state.code);
      if (await copyText(`https://www.peerway.me/share?code=${code}`)) {
        toast('Enlace copiado. Al abrirlo se muestra el código.', 'success');
      }
    });
    $('#toggleCode').addEventListener('click', () => {
      codeVisible = !codeVisible;
      renderCode();
    });
    $('#rotateCodeBtn').addEventListener('click', async () => {
      const ok = window.confirm(isEnglish()
        ? 'Rotate your code? Whoever has the current one will not reach you until you share the new one.'
        : '¿Rotar tu código? Quien tenga el actual no podrá encontrarte hasta que compartas el nuevo.');
      if (!ok) return;
      try {
        await api('/api/code/rotate', { method: 'POST' });
        codeVisible = true;
        toast(isEnglish() ? 'Code rotated.' : 'Código rotado.', 'success');
        await loadState();
      } catch (error) {
        toast(error.message, 'error');
      }
    });
    $('#openFolderBtn').addEventListener('click', async () => {
      try { await api('/api/open-folder', { method: 'POST' }); }
      catch (error) { toast(error.message, 'error'); }
    });
    $('#openReceivedFolder').addEventListener('click', async () => {
      try { await api('/api/open-folder', { method: 'POST' }); }
      catch (error) { toast(error.message, 'error'); }
    });
    $('#aboutFolder').addEventListener('click', async () => {
      try { await api('/api/open-folder', { method: 'POST' }); }
      catch (error) { toast(error.message, 'error'); }
    });

    const codeInput = $('#targetCode');
    codeInput.addEventListener('input', () => {
      const formatted = formatCode(codeInput.value);
      codeInput.value = formatted;
      const valid = normalizeCode(formatted).length === 8;
      $('#codeOk').classList.toggle('hidden', !valid);
      $('#sendBtn').disabled = !canSend();
    });
    const commitRecipient = () => {
      const value = codeInput.value.trim();
      if (!value) return;
      if (addRecipient(value)) {
        codeInput.value = '';
        $('#codeOk').classList.add('hidden');
      } else {
        toast('El código debe tener 8 caracteres.', 'warning');
      }
    };
    $('#addRecipient').addEventListener('click', commitRecipient);
    codeInput.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        commitRecipient();
      }
    });

    // A peerway:// link opens the app at /?to=CODE and selects that recipient.
    (function applyDeepLink() {
      const url = new URL(window.location.href);
      const raw = url.searchParams.get('to');
      if (!raw) return;
      url.searchParams.delete('to');
      window.history.replaceState({}, '', url.pathname + url.search + url.hash);
      const code = normalizeCode(raw);
      if (code.length !== 8) return;
      setView('send');
      addRecipient(code);
    })();

    $('#pickBtn').addEventListener('click', (event) => { event.stopPropagation(); pickFiles(); });
    $('#localFileInput').addEventListener('change', (event) => {
      const files = Array.from(event.target.files || []);
      event.target.value = '';
      if (files.length) stageFiles(files);
    });
    $('#dropzone').addEventListener('click', (event) => {
      if (event.target.closest('#pickBtn') || event.target.closest('#localFileInput')) return;
      pickFiles();
    });
    $('#dropzone').addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); pickFiles(); }
    });
    const dropzone = $('#dropzone');
    ['dragenter', 'dragover'].forEach((type) => dropzone.addEventListener(type, (event) => {
      event.preventDefault();
      dropzone.classList.add('drag');
    }));
    ['dragleave', 'drop'].forEach((type) => dropzone.addEventListener(type, (event) => {
      event.preventDefault();
      if (type === 'dragleave' && event.relatedTarget && dropzone.contains(event.relatedTarget)) return;
      dropzone.classList.remove('drag');
    }));
    dropzone.addEventListener('drop', (event) => {
      const files = Array.from(event.dataTransfer?.files || []);
      if (files.length) stageFiles(files);
    });
    $('#clearFiles').addEventListener('click', () => { selected = []; renderSelected(); });
    $('#sendBtn').addEventListener('click', sendSelected);
    $('#shareWebBtn').addEventListener('click', async () => {
      if (!selected.length) return;
      try {
        const result = await api('/api/share', {
          method: 'POST',
          body: JSON.stringify({ paths: selected.map((file) => file.path), hours: 24 }),
        });
        $('#webShareUrl').value = result.url;
        $('#webShare').classList.remove('hidden');
        if (await copyText(result.url)) {
          toast('Enlace web creado y copiado. Solo funciona en tu red local.', 'success');
        } else {
          toast('Enlace web creado.', 'success');
        }
      } catch (error) {
        toast(error.message, 'error');
      }
    });
    $('#copyWebShare').addEventListener('click', async () => {
      if (await copyText($('#webShareUrl').value)) toast('Enlace copiado.', 'success');
    });
    $('#refreshTransfers').addEventListener('click', loadState);
    $('#clearHistory').addEventListener('click', async () => {
      if (!confirm(isEnglish() ? 'Delete all history? Files are not deleted.' : '¿Borrar todo el historial? Los archivos no se borran.')) return;
      try {
        await api('/api/history/clear', { method: 'POST' });
        toast(isEnglish() ? 'History cleared.' : 'Historial borrado.', 'info');
        await loadState();
      } catch (error) { toast(error.message, 'error'); }
    });
    $('#saveDisplayName').addEventListener('click', saveDisplayName);
    ['cloudEmail', 'cloudDisplayName'].forEach((id) => {
      const input = document.getElementById(id);
      input?.addEventListener('input', () => { input.dataset.dirty = '1'; });
    });
    $('#resendVerify').addEventListener('click', async () => {
      const email = $('#cloudEmail').value.trim();
      if (!email) {
        toast(isEnglish() ? 'Write your email first.' : 'Escribe tu correo primero.', 'warning');
        return;
      }
      try {
        await api('/api/account/email', { method: 'POST', body: JSON.stringify({ email }) });
        $('#cloudEmail').dataset.dirty = '0';
        toast(isEnglish()
          ? 'Confirmation link sent.'
          : 'Te enviamos un enlace de confirmación.', 'success');
        await loadState();
      } catch (error) {
        toast(error.message, 'error');
      }
    });
    $('#cloudDisplayName').addEventListener('keydown', (event) => {
      if (event.key === 'Enter') { event.preventDefault(); saveDisplayName(); }
    });

    $('#declineOffer').addEventListener('click', () => decideOffer(false));
    $('#acceptOffer').addEventListener('click', () => decideOffer(true));

    $$('#authTabs button').forEach((button) => button.addEventListener('click', () => {
      authMode = button.dataset.auth;
      showAuthError('');
      $$('#authTabs button').forEach((b) => b.classList.toggle('active', b === button));
      $('#authSubmit').textContent = authMode === 'register' ? 'Crear cuenta' : 'Iniciar sesión';
      $('#authPass').autocomplete = authMode === 'register' ? 'new-password' : 'current-password';
      $('#authEmailBlock').classList.toggle('hidden', authMode !== 'register');
      $('#authNote').textContent = authMode === 'register'
        ? 'Tu identidad criptográfica se crea en este equipo. La contraseña solo protege tu cuenta.'
        : 'Inicia sesión para recuperar tu código permanente y tu nube.';
    }));
    $('#authForm').addEventListener('submit', authenticate);
    ['authUser', 'authPass', 'authEmail'].forEach((id) => {
      document.getElementById(id)?.addEventListener('input', () => showAuthError(''));
    });
    $('#forgotPassword').addEventListener('click', async () => {
      const block = $('#authEmailBlock');
      const input = $('#authEmail');
      block.classList.remove('hidden');
      const email = input.value.trim();
      if (!email) {
        $('#authNote').textContent = 'Escribe tu correo y vuelve a pulsar «¿Olvidaste tu contraseña?».';
        input.focus();
        return;
      }
      try {
        await api('/api/auth/reset', { method: 'POST', body: JSON.stringify({ email }) });
        toast(isEnglish()
          ? 'If that email has an account, we sent a reset link.'
          : 'Si ese correo tiene cuenta, te enviamos un enlace.', 'success');
      } catch (error) {
        toast(error.message, 'error');
      }
    });
    $('#cloudLogout').addEventListener('click', async () => {
      try {
        await api('/api/auth/logout', { method: 'POST' });
        cloudData = null;
        toast('Sesión cerrada.', 'info');
        await loadState();
      } catch (error) { toast(error.message, 'error'); }
    });
    $('#activateBtn').addEventListener('click', activateLicense);
    $('#cloudRefresh').addEventListener('click', loadCloud);
    $('#cloudUploadBtn').addEventListener('click', () => $('#cloudFileInput').click());
    $('#cloudFileInput').addEventListener('change', async (event) => {
      const file = event.target.files?.[0];
      event.target.value = '';
      if (file) await uploadCloudFile(file);
    });

    const markDirty = () => { settingsDirty = true; };
    ['setDevice', 'setDownload', 'setAutoTrust', 'setLan', 'setAutoUpdate', 'setChangelog', 'setAutostart', 'setTelemetry', 'setNotifications', 'setNotifyUpdates', 'setNotifyTransfers']
      .forEach((id) => document.getElementById(id).addEventListener('input', markDirty));
    $('#setNotifications').addEventListener('change', syncNotificationControls);
    $('#saveSettings').addEventListener('click', saveSettings);
    $('#speedPlans')?.addEventListener('click', () => setView('cloud'));
    $('#addTrust').addEventListener('click', addTrusted);
    $('#toggleTrustedCodes').addEventListener('click', () => {
      trustedCodesVisible = !trustedCodesVisible;
      renderTrusted(state || {});
    });
    $('#trustCode').addEventListener('input', (event) => {
      event.target.value = formatCode(event.target.value);
    });
    $('#trustCode').addEventListener('keydown', (event) => {
      if (event.key === 'Enter') { event.preventDefault(); addTrusted(); }
    });

    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) loadState();
      reportVisibility();
    });
    window.addEventListener('beforeunload', () => { if (ws) ws.close(); });
    window.addEventListener('resize', applyHomeGrid);
    window.addEventListener('peerway-toast', (event) => {
      if (event.detail?.message) toast(event.detail.message, event.detail.level || 'info');
    });
    window.addEventListener('peerway-update', (event) => {
      if (event.detail?.version) {
        showUpdate(event.detail.version, !!event.detail.security);
        lastUpdateCheck = {
          at: Date.now(),
          result: { version: event.detail.version, security: !!event.detail.security },
          error: null,
        };
      }
    });
    window.addEventListener('peerway-update-error', (event) => {
      if (event.detail?.message) showUpdateError(event.detail.message);
    });
    $('#notifyBtn').addEventListener('click', (event) => {
      event.stopPropagation();
      toggleNotifyPanel();
    });
    $('#notifyClear').addEventListener('click', (event) => {
      event.stopPropagation();
      clearNotifications();
    });
    document.addEventListener('click', (event) => {
      if (!event.target.closest('.topbar-actions')) toggleNotifyPanel(false);
    });
    $('#consentAccept').addEventListener('click', () => chooseConsent(true));
    $('#consentReject').addEventListener('click', () => chooseConsent(false));
    $('#guideClose').addEventListener('click', hideGuide);
    $('#guideDismiss').addEventListener('click', hideGuide);
    $('#showGuide').addEventListener('click', showGuide);
    $('#revokeOthers').addEventListener('click', revokeOtherSessions);
    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && !$('#guideOverlay').classList.contains('hidden')) hideGuide();
      if (event.key !== 'Tab') return;
      const overlay = [$('#offerOverlay'), $('#guideOverlay')].find(
        (el) => el && !el.classList.contains('hidden')
      );
      if (!overlay) return;
      const focusable = Array.from(
        overlay.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')
      ).filter((el) => !el.disabled && el.offsetParent !== null);
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    });
    document.addEventListener('peerway-language', (event) => {
      persistLanguage(event.detail?.lang);
      setView(currentView);
      applyPlans();
      if (state?.logged_in) {
        renderCloudAccount(state);
        renderCloudFiles();
      }
    });
    $('#updateInstallBtn').addEventListener('click', installUpdateFromWindow);
    $('#updateLaterBtn').addEventListener('click', hideUpdate);
    $('#checkUpdateBtn').addEventListener('click', checkForUpdatesFromWindow);
  }

  async function init() {
    bindEvents();
    reportVisibility();
    setupWindowControls();
    setView(currentView);
    renderNotifications();
    maybeShowGuide();
    await Promise.all([loadState(), loadPlans()]);
    if (state?.logged_in) loadCloud();
    const uiLanguage = window.PeerWayI18n?.lang;
    if (state && (uiLanguage === 'es' || uiLanguage === 'en') && state.language !== uiLanguage) {
      persistLanguage(uiLanguage);
    }
    const invoke = tauriInvoke();
    if (typeof invoke === 'function') {
      $('#checkUpdateBtn').classList.remove('hidden');
      invoke('pending_update')
        .then((info) => {
          if (!info) return;
          if (typeof info === 'string') showUpdate(info);
          else if (info.version) showUpdate(info.version, !!info.security);
          lastUpdateCheck = {
            at: Date.now(),
            result: info && typeof info === 'object'
              ? { version: info.version, security: !!info.security }
              : (info ? { version: info, security: false } : null),
            error: null,
          };
        })
        .catch(() => {});
    }
    connectEvents();
    loadRecentRecipients();
    setInterval(() => {
      if (!document.hidden) loadState();
    }, 6000);
    setInterval(() => {
      if (!document.hidden && currentView === 'cloud' && state?.logged_in) loadCloud();
    }, 15000);
    setInterval(loadPlans, 60000);
  }

  init();
})();
