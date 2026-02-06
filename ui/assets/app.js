// hookbin dashboard — live & kinetic
'use strict';

// --- API Client ---
let _apiActive = 0;
function apiSpinner(on) {
  _apiActive += on ? 1 : -1;
  if (_apiActive < 0) _apiActive = 0;
  const el = document.getElementById('activity-spinner');
  if (el) el.classList.toggle('active', _apiActive > 0);
}

const api = {
  async get(path) {
    apiSpinner(true);
    try {
      const res = await fetch(path);
      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: res.statusText }));
        throw { status: res.status, ...err };
      }
      return res.json();
    } finally {
      apiSpinner(false);
    }
  },

  async post(path, body) {
    apiSpinner(true);
    try {
      const res = await fetch(path, {
        method: 'POST',
        headers: body ? { 'Content-Type': 'application/json' } : {},
        body: body ? JSON.stringify(body) : undefined,
      });
      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: res.statusText }));
        throw { status: res.status, ...err };
      }
      return res.json();
    } finally {
      apiSpinner(false);
    }
  },

  async del(path) {
    apiSpinner(true);
    try {
      const res = await fetch(path, { method: 'DELETE' });
      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: res.statusText }));
        throw { status: res.status, ...err };
      }
      return res.json();
    } finally {
      apiSpinner(false);
    }
  },
};

// --- Utilities ---
function $(sel) { return document.querySelector(sel); }
function show(id) {
  document.querySelectorAll('.view').forEach(v => v.classList.remove('active'));
  const el = document.getElementById(id);
  if (el) el.classList.add('active');
}

function timeAgo(epoch) {
  const seconds = Math.floor(Date.now() / 1000) - epoch;
  if (seconds < 0) return 'just now';
  if (seconds < 60) return seconds + 's ago';
  if (seconds < 3600) return Math.floor(seconds / 60) + 'm ago';
  if (seconds < 86400) return Math.floor(seconds / 3600) + 'h ago';
  return Math.floor(seconds / 86400) + 'd ago';
}

function formatBytes(n) {
  if (n === 0) return '0 B';
  if (n < 1024) return n + ' B';
  if (n < 1048576) return (n / 1024).toFixed(1) + ' KB';
  return (n / 1048576).toFixed(1) + ' MB';
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function toast(msg, type) {
  const el = document.createElement('div');
  el.className = 'toast ' + (type || '');
  el.textContent = msg;
  document.body.appendChild(el);
  setTimeout(() => {
    el.classList.add('removing');
    setTimeout(() => el.remove(), 200);
  }, 2300);
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast('Copied!', 'success');
  } catch {
    toast('Copy failed', 'error');
  }
}

function methodBadge(method) {
  return '<span class="method-badge method-' + escapeHtml(method) + '">' + escapeHtml(method) + '</span>';
}

// --- Skeleton Loaders ---
function skeletonCards(n) {
  let html = '';
  for (let i = 0; i < n; i++) {
    html += '<div class="skeleton">';
    html += '<div class="skeleton-line medium"></div>';
    html += '<div class="skeleton-line short"></div>';
    html += '</div>';
  }
  return html;
}

function highlightJson(escaped) {
  // Input is HTML-escaped (& < > encoded, but " is NOT encoded)
  return escaped.replace(
    /("(?:[^"\\]|\\.)*")\s*:|("(?:[^"\\]|\\.)*")|\b(true|false)\b|\b(null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g,
    function(m, key, str, bool, nul, num) {
      if (key) return '<span class="json-key">' + key + '</span>:';
      if (str) return '<span class="json-string">' + str + '</span>';
      if (bool) return '<span class="json-bool">' + bool + '</span>';
      if (nul) return '<span class="json-null">' + nul + '</span>';
      if (num) return '<span class="json-number">' + num + '</span>';
      return m;
    }
  );
}

function codeBlockWithLines(content, highlighted) {
  const lines = content.split('\n');
  let nums = '';
  for (let i = 1; i <= lines.length; i++) {
    nums += '<span>' + i + '</span>';
  }
  return '<div class="code-block-wrapper">' +
    '<div class="code-line-numbers">' + nums + '</div>' +
    '<pre>' + (highlighted || escapeHtml(content)) + '</pre>' +
    '</div>';
}

// --- Polling Manager ---
const poller = {
  _intervals: [],

  start(fn, ms) {
    const id = setInterval(fn, ms);
    this._intervals.push(id);
    return id;
  },

  clear() {
    this._intervals.forEach(clearInterval);
    this._intervals = [];
  },
};

// --- Relative Time Ticker ---
// Updates all elements with data-epoch attribute every second
function startTimeTicker() {
  return poller.start(() => {
    document.querySelectorAll('[data-epoch]').forEach(el => {
      const epoch = parseInt(el.dataset.epoch, 10);
      if (!isNaN(epoch)) el.textContent = timeAgo(epoch);
    });
  }, 1000);
}

// --- Router ---
function getRoute() {
  return location.hash.slice(1) || '/hooks';
}

function navigate(path) {
  location.hash = '#' + path;
}

function parseRoute(hash) {
  const hookDetail = hash.match(/^\/hooks\/([^/]+)\/requests\/([^/]+)$/);
  if (hookDetail) return { view: 'detail', hookId: hookDetail[1], requestId: hookDetail[2] };

  const hookRequests = hash.match(/^\/hooks\/([^/]+)$/);
  if (hookRequests) return { view: 'requests', hookId: hookRequests[1] };

  return { view: 'hooks' };
}

async function route() {
  poller.clear();
  const parsed = parseRoute(getRoute());
  try {
    if (parsed.view === 'hooks') {
      await renderHookList();
    } else if (parsed.view === 'requests') {
      await renderRequestFeed(parsed.hookId);
    } else if (parsed.view === 'detail') {
      await renderRequestDetail(parsed.hookId, parsed.requestId);
    }
  } catch (err) {
    console.error('Route error:', err);
    show('view-hooks');
    const el = $('#view-hooks');
    el.innerHTML = '<p class="empty-state">Error: ' + escapeHtml(err.error || err.message || 'Unknown error') + '</p>';
  }
}

// --- Health Check ---
async function checkHealth() {
  const indicator = $('#health-status');
  try {
    const data = await api.get('/health');
    indicator.className = 'health-indicator ok';
    indicator.title = 'v' + data.version + ' — up ' + data.uptime_seconds + 's';
  } catch {
    indicator.className = 'health-indicator error';
    indicator.title = 'Unreachable';
  }
}

// --- Hook List View ---
let _createFormOpen = false;

async function renderHookList() {
  show('view-hooks');
  const el = $('#view-hooks');
  el.innerHTML = '<div class="toolbar"><h2 style="margin:0;font-size:16px;">Hooks</h2></div>' + skeletonCards(3);

  const data = await api.get('/api/hooks');
  const hooks = data.hooks || [];
  const origin = location.origin;

  let html = '<div class="toolbar">';
  html += '<h2 style="margin:0;font-size:16px;">Hooks (' + data.count + ')</h2>';
  html += '<button class="btn btn-primary" id="btn-create-hook">+ Create Hook</button>';
  html += '</div>';

  // Inline create form
  html += '<div class="inline-form" id="create-form">';
  html += '<div class="inline-form-inner">';
  html += '<input type="text" id="create-hook-name" placeholder="Hook name (blank for auto)" maxlength="100" autocomplete="off">';
  html += '<button class="btn btn-primary btn-small" id="btn-create-submit">Create</button>';
  html += '<button class="btn btn-small" id="btn-create-cancel">Cancel</button>';
  html += '</div>';
  html += '</div>';

  if (hooks.length === 0) {
    html += '<div class="empty-state">';
    html += '<div class="empty-icon">{...}</div>';
    html += '<div>No hooks yet. Create one to start capturing webhooks.</div>';
    html += '</div>';
  } else {
    for (const hook of hooks) {
      const url = origin + hook.url;
      html += '<div class="card card-clickable" data-hook-id="' + escapeHtml(hook.hook_id) + '">';
      html += '<div class="card-header">';
      html += '<span class="card-title">' + escapeHtml(hook.name) + '</span>';
      html += '<span style="display:flex;gap:6px;flex-shrink:0;align-items:center;">';
      html += '<span class="badge-count">' + hook.request_count + ' req' + (hook.request_count !== 1 ? 's' : '') + '</span>';
      html += '<button class="btn btn-small btn-copy-url" data-url="' + escapeHtml(url) + '" title="Copy URL">copy</button>';
      html += '<button class="btn btn-small btn-danger btn-delete-hook" data-hook-id="' + escapeHtml(hook.hook_id) + '" data-hook-name="' + escapeHtml(hook.name) + '" title="Delete">del</button>';
      html += '<span class="delete-confirm" id="del-confirm-' + escapeHtml(hook.hook_id) + '">';
      html += '<span class="delete-confirm-text">sure?</span>';
      html += '<button class="btn btn-small btn-danger btn-delete-yes" data-hook-id="' + escapeHtml(hook.hook_id) + '">yes</button>';
      html += '<button class="btn btn-small btn-delete-no" data-hook-id="' + escapeHtml(hook.hook_id) + '">no</button>';
      html += '</span>';
      html += '</span>';
      html += '</div>';
      html += '<div class="card-meta">';
      html += '<span class="url-box" style="border:0;padding:0;background:none;font-size:12px;">' + escapeHtml(url) + '</span>';
      html += '<span data-epoch="' + hook.created_at + '">' + timeAgo(hook.created_at) + '</span>';
      html += '</div>';
      html += '</div>';
    }
  }

  el.innerHTML = html;

  // Wire up create form
  const createBtn = document.getElementById('btn-create-hook');
  const createForm = document.getElementById('create-form');
  const nameInput = document.getElementById('create-hook-name');

  function openCreateForm() {
    _createFormOpen = true;
    createForm.classList.add('open');
    nameInput.value = '';
    setTimeout(() => nameInput.focus(), 50);
  }

  function closeCreateForm() {
    _createFormOpen = false;
    createForm.classList.remove('open');
  }

  async function submitCreateForm() {
    const name = nameInput.value.trim();
    const body = name ? { name } : {};
    const submitBtn = document.getElementById('btn-create-submit');
    submitBtn.classList.add('loading');
    submitBtn.textContent = '...';
    try {
      const hook = await api.post('/api/hooks', body);
      closeCreateForm();
      toast('Hook created', 'success');
      navigate('/hooks/' + hook.hook_id);
    } catch (err) {
      toast(err.error || 'Failed to create hook', 'error');
      submitBtn.classList.remove('loading');
      submitBtn.textContent = 'Create';
    }
  }

  if (createBtn) {
    createBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      if (_createFormOpen) closeCreateForm();
      else openCreateForm();
    });
  }

  if (nameInput) {
    nameInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); submitCreateForm(); }
      if (e.key === 'Escape') { e.preventDefault(); closeCreateForm(); }
    });
  }

  const submitBtn = document.getElementById('btn-create-submit');
  if (submitBtn) submitBtn.addEventListener('click', (e) => { e.stopPropagation(); submitCreateForm(); });
  const cancelBtn = document.getElementById('btn-create-cancel');
  if (cancelBtn) cancelBtn.addEventListener('click', (e) => { e.stopPropagation(); closeCreateForm(); });

  // Copy URL handlers
  el.querySelectorAll('.btn-copy-url').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      copyText(btn.dataset.url);
    });
  });

  // Delete hook handlers — inline confirmation
  el.querySelectorAll('.btn-delete-hook').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const hookId = btn.dataset.hookId;
      const confirm = document.getElementById('del-confirm-' + hookId);
      if (confirm) {
        btn.style.display = 'none';
        confirm.classList.add('open');
      }
    });
  });

  el.querySelectorAll('.btn-delete-yes').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const hookId = btn.dataset.hookId;
      try {
        await api.del('/api/hooks/' + hookId);
        toast('Hook deleted', 'success');
        await renderHookList();
      } catch (err) {
        toast(err.error || 'Failed to delete hook', 'error');
      }
    });
  });

  el.querySelectorAll('.btn-delete-no').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const hookId = btn.dataset.hookId;
      const confirm = document.getElementById('del-confirm-' + hookId);
      if (confirm) confirm.classList.remove('open');
      // Re-show delete button
      const delBtn = el.querySelector('.btn-delete-hook[data-hook-id="' + hookId + '"]');
      if (delBtn) delBtn.style.display = '';
    });
  });

  // Click card to navigate
  el.querySelectorAll('.card-clickable').forEach(card => {
    card.addEventListener('click', () => {
      navigate('/hooks/' + card.dataset.hookId);
    });
  });

  // Start polling to keep request counts fresh
  startTimeTicker();
  pollHookList(hooks);
}

function pollHookList(prevHooks) {
  const prevCounts = {};
  for (const h of prevHooks) prevCounts[h.hook_id] = h.request_count;

  poller.start(async () => {
    try {
      const data = await api.get('/api/hooks');
      const hooks = data.hooks || [];
      let changed = hooks.length !== prevHooks.length;
      if (!changed) {
        for (const h of hooks) {
          if (prevCounts[h.hook_id] !== h.request_count) { changed = true; break; }
        }
      }
      if (changed) {
        // Update counts in-place instead of full re-render
        for (const h of hooks) {
          const card = document.querySelector('.card-clickable[data-hook-id="' + h.hook_id + '"]');
          if (card) {
            const badge = card.querySelector('.badge-count');
            if (badge && prevCounts[h.hook_id] !== h.request_count) {
              badge.textContent = h.request_count + ' req' + (h.request_count !== 1 ? 's' : '');
            }
          }
        }
        // Update prev for next cycle
        prevHooks = hooks;
        for (const h of hooks) prevCounts[h.hook_id] = h.request_count;
      }
    } catch {
      // Silently ignore polling errors
    }
  }, 5000);
}

// --- Request Feed View ---
async function renderRequestFeed(hookId) {
  show('view-requests');
  const el = $('#view-requests');
  el.innerHTML = '<div class="breadcrumb"><a href="#/hooks">Hooks</a> / ...</div>' + skeletonCards(4);

  const hook = await api.get('/api/hooks/' + hookId);
  const origin = location.origin;
  const ingestUrl = origin + hook.url;

  let offset = 0;
  const limit = 50;
  const seenIds = new Set();
  let lastKnownCount = hook.request_count;
  let newCount = 0;

  async function loadRequests(append) {
    const data = await api.get('/api/hooks/' + hookId + '/requests?limit=' + limit + '&offset=' + offset);
    const requests = data.requests || [];
    const total = data.total || 0;

    if (!append) {
      let html = '<div class="breadcrumb"><a href="#/hooks">Hooks</a> / ' + escapeHtml(hook.name) + '</div>';
      html += '<div class="url-box" style="margin-bottom:16px;">';
      html += '<span style="flex:1;">' + escapeHtml(ingestUrl) + '</span>';
      html += '<button class="btn btn-small btn-copy-feed-url" data-url="' + escapeHtml(ingestUrl) + '">copy</button>';
      html += '</div>';
      html += '<div class="toolbar">';
      html += '<span class="status-text" id="feed-status"></span>';
      html += '<span style="display:flex;align-items:center;gap:8px;">';
      html += '<span id="new-badge-container"></span>';
      html += '<span class="live-dot" title="Live polling"></span>';
      html += '</span>';
      html += '</div>';
      html += '<div id="request-list"></div>';
      html += '<div id="feed-footer" style="text-align:center;margin:16px 0;"></div>';
      el.innerHTML = html;

      el.querySelector('.btn-copy-feed-url').addEventListener('click', (e) => {
        e.stopPropagation();
        copyText(e.target.dataset.url);
      });
    }

    const listEl = document.getElementById('request-list');
    const statusEl = document.getElementById('feed-status');
    const footerEl = document.getElementById('feed-footer');

    // Track seen IDs
    for (const req of requests) seenIds.add(req.request_id);

    const currentCount = listEl.querySelectorAll('.card').length + requests.length;
    statusEl.textContent = 'Showing ' + currentCount + ' of ' + total;

    if (total === 0 && !append) {
      listEl.innerHTML = '<div class="empty-state">' +
        '<div class="empty-icon">{ }</div>' +
        '<div>No requests captured yet.</div>' +
        '<div style="margin-top:8px;font-size:12px;">Send a webhook to <span style="color:var(--accent);">' + escapeHtml(ingestUrl) + '</span></div>' +
        '<div class="listening-dots" style="margin-top:12px;">Listening for webhooks</div>' +
        '</div>';
      footerEl.innerHTML = '';
      lastKnownCount = 0;
      return;
    }

    let cards = '';
    for (const req of requests) {
      cards += requestCard(req, hookId, false);
    }

    if (append) {
      listEl.insertAdjacentHTML('beforeend', cards);
    } else {
      listEl.innerHTML = cards;
    }

    bindRequestCards(listEl);
    lastKnownCount = total;

    // Load more
    if (currentCount < total) {
      footerEl.innerHTML = '<button class="btn" id="btn-load-more">Load more</button>';
      document.getElementById('btn-load-more').addEventListener('click', async () => {
        offset += limit;
        try { await loadRequests(true); }
        catch (err) { toast(err.error || 'Failed to load more', 'error'); }
      });
    } else {
      footerEl.innerHTML = '';
    }
  }

  await loadRequests(false);

  // Start live polling
  startTimeTicker();
  poller.start(async () => {
    try {
      const hookData = await api.get('/api/hooks/' + hookId);
      const currentTotal = hookData.request_count;

      if (currentTotal > lastKnownCount) {
        const diff = currentTotal - lastKnownCount;
        // Fetch the newest requests
        const data = await api.get('/api/hooks/' + hookId + '/requests?limit=' + diff + '&offset=0');
        const newReqs = (data.requests || []).filter(r => !seenIds.has(r.request_id));

        if (newReqs.length > 0) {
          const listEl = document.getElementById('request-list');
          const statusEl = document.getElementById('feed-status');

          // Clear empty state if it was showing
          const emptyEl = listEl.querySelector('.empty-state');
          if (emptyEl) listEl.innerHTML = '';

          // Prepend new cards with animation
          let newHtml = '';
          for (const req of newReqs) {
            seenIds.add(req.request_id);
            newHtml += requestCard(req, hookId, true);
          }

          listEl.insertAdjacentHTML('afterbegin', newHtml);
          bindRequestCards(listEl);

          lastKnownCount = currentTotal;
          const totalCards = listEl.querySelectorAll('.card').length;
          statusEl.textContent = 'Showing ' + totalCards + ' of ' + currentTotal;

          // Show new badge if scrolled down
          if (window.scrollY > 200) {
            newCount += newReqs.length;
            showNewBadge(newCount);
          }
        } else {
          lastKnownCount = currentTotal;
        }
      }
    } catch {
      // Silently ignore polling errors
    }
  }, 3000);
}

function requestCard(req, hookId, isNew) {
  const cls = 'card card-clickable' + (isNew ? ' card-new' : '');
  let html = '<div class="' + cls + '" data-hook-id="' + escapeHtml(hookId) + '" data-request-id="' + escapeHtml(req.request_id) + '">';
  html += '<div class="card-header">';
  html += '<span>' + methodBadge(req.method) + ' <span style="color:var(--text-muted);">' + escapeHtml(req.path) + '</span></span>';
  html += '<span class="card-meta" style="margin:0;" data-epoch="' + req.received_at + '">' + timeAgo(req.received_at) + '</span>';
  html += '</div>';
  html += '<div class="card-meta">';
  html += '<span>' + formatBytes(req.content_length) + '</span>';
  html += '<span>' + escapeHtml(req.source_ip) + '</span>';
  html += '</div>';
  html += '</div>';
  return html;
}

function bindRequestCards(container) {
  container.querySelectorAll('.card-clickable').forEach(card => {
    if (!card._bound) {
      card._bound = true;
      card.onclick = () => {
        navigate('/hooks/' + card.dataset.hookId + '/requests/' + card.dataset.requestId);
      };
    }
  });
}

function showNewBadge(count) {
  const container = document.getElementById('new-badge-container');
  if (!container) return;
  container.innerHTML = '<span class="new-badge" id="new-badge">' + count + ' new</span>';
  document.getElementById('new-badge').addEventListener('click', () => {
    window.scrollTo({ top: 0, behavior: 'smooth' });
    container.innerHTML = '';
    count = 0;
  });
}

// --- Request Inspector View ---
async function renderRequestDetail(hookId, requestId) {
  show('view-detail');
  const el = $('#view-detail');
  el.innerHTML = '<div class="breadcrumb"><a href="#/hooks">Hooks</a> / <a href="#/hooks/' + escapeHtml(hookId) + '">Requests</a> / ...</div>' + skeletonCards(3);

  const req = await api.get('/api/hooks/' + hookId + '/requests/' + requestId);

  // Decode body from base64
  let bodyText = '';
  let bodyRaw = '';
  let isJson = false;
  let isBinary = false;
  if (!req.body || req.body === '') {
    // empty
  } else {
    try {
      const raw = atob(req.body);
      const bytes = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
      bodyText = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
      bodyRaw = bodyText;

      try {
        const parsed = JSON.parse(bodyText);
        bodyText = JSON.stringify(parsed, null, 2);
        isJson = true;
      } catch {
        // Not JSON, keep as-is
      }
    } catch {
      isBinary = true;
    }
  }

  // Format timestamp
  const date = new Date(req.received_at * 1000);
  const timestamp = date.toISOString().replace('T', ' ').replace(/\..*$/, '') + ' UTC';

  let html = '<div class="breadcrumb">';
  html += '<a href="#/hooks">Hooks</a> / ';
  html += '<a href="#/hooks/' + escapeHtml(hookId) + '">Requests</a> / ';
  html += escapeHtml(requestId.substring(0, 8)) + '...';
  html += '</div>';

  // Summary card
  html += '<div class="card" style="margin-bottom:16px;">';
  html += '<div class="card-header">';
  html += '<span>' + methodBadge(req.method) + ' <span style="color:var(--text-muted);">' + escapeHtml(req.path) + '</span></span>';
  html += '</div>';
  html += '<div class="card-meta">';
  html += '<span>' + formatBytes(req.content_length) + '</span>';
  html += '<span>' + escapeHtml(req.source_ip) + '</span>';
  html += '<span>' + escapeHtml(timestamp) + '</span>';
  html += '<span data-epoch="' + req.received_at + '">' + timeAgo(req.received_at) + '</span>';
  html += '</div>';
  html += '</div>';

  // Headers — collapsible
  const headers = req.headers || {};
  const headerKeys = Object.keys(headers);
  html += '<div class="section-header">';
  html += '<span class="section-toggle" id="toggle-headers"><span class="section-title">Headers (' + headerKeys.length + ')</span></span>';
  if (headerKeys.length > 0) {
    html += '<button class="btn btn-small" id="btn-copy-headers">copy</button>';
  }
  html += '</div>';

  html += '<div class="section-content" id="section-headers">';
  if (headerKeys.length > 0) {
    html += '<table class="kv-table">';
    for (const key of headerKeys.sort()) {
      html += '<tr><th>' + escapeHtml(key) + '</th><td>' + escapeHtml(headers[key]) + '</td></tr>';
    }
    html += '</table>';
  } else {
    html += '<p style="color:var(--text-muted);font-size:13px;">(no headers)</p>';
  }
  html += '</div>';

  // Body
  html += '<div class="section-header" style="margin-top:20px;">';
  html += '<span class="section-toggle" id="toggle-body"><span class="section-title">Body</span></span>';
  html += '<span style="display:flex;gap:6px;align-items:center;">';
  if (bodyText && !isBinary) {
    html += '<button class="btn btn-small" id="btn-copy-body">copy</button>';
    if (isJson) {
      html += '<div class="toggle-group">';
      html += '<button class="toggle-option active" id="btn-pretty">Pretty</button>';
      html += '<button class="toggle-option" id="btn-raw">Raw</button>';
      html += '</div>';
    }
  }
  html += '</span>';
  html += '</div>';

  html += '<div class="section-content" id="section-body">';
  if (!req.body || req.body === '') {
    html += '<span style="color:var(--text-muted);">(empty body)</span>';
  } else if (isBinary) {
    html += '<span style="color:var(--text-muted);">(binary payload, ' + formatBytes(req.content_length) + ')</span>';
  } else if (isJson) {
    html += '<div id="body-pretty">' + codeBlockWithLines(bodyText, highlightJson(escapeHtml(bodyText))) + '</div>';
    html += '<div id="body-raw" style="display:none;">' + codeBlockWithLines(bodyRaw) + '</div>';
  } else {
    html += codeBlockWithLines(bodyText);
  }
  html += '</div>';

  el.innerHTML = html;

  // Collapsible headers
  const toggleHeaders = document.getElementById('toggle-headers');
  if (toggleHeaders) {
    toggleHeaders.addEventListener('click', () => {
      toggleHeaders.classList.toggle('collapsed');
      document.getElementById('section-headers').classList.toggle('collapsed');
    });
  }

  // Collapsible body
  const toggleBody = document.getElementById('toggle-body');
  if (toggleBody) {
    toggleBody.addEventListener('click', () => {
      toggleBody.classList.toggle('collapsed');
      document.getElementById('section-body').classList.toggle('collapsed');
    });
  }

  // Copy headers handler
  const copyHeadersBtn = document.getElementById('btn-copy-headers');
  if (copyHeadersBtn) {
    copyHeadersBtn.addEventListener('click', () => {
      const text = headerKeys.sort().map(k => k + ': ' + headers[k]).join('\n');
      copyText(text);
    });
  }

  // Copy body handler
  const copyBodyBtn = document.getElementById('btn-copy-body');
  if (copyBodyBtn) {
    copyBodyBtn.addEventListener('click', () => {
      copyText(bodyRaw || bodyText);
    });
  }

  // Pretty/Raw toggle
  const prettyBtn = document.getElementById('btn-pretty');
  const rawBtn = document.getElementById('btn-raw');
  if (prettyBtn && rawBtn) {
    prettyBtn.addEventListener('click', () => {
      prettyBtn.classList.add('active');
      rawBtn.classList.remove('active');
      document.getElementById('body-pretty').style.display = '';
      document.getElementById('body-raw').style.display = 'none';
    });
    rawBtn.addEventListener('click', () => {
      rawBtn.classList.add('active');
      prettyBtn.classList.remove('active');
      document.getElementById('body-raw').style.display = '';
      document.getElementById('body-pretty').style.display = 'none';
    });
  }

  // Start time ticker for detail view
  startTimeTicker();
}

// --- Init ---
window.addEventListener('hashchange', route);
window.addEventListener('DOMContentLoaded', () => {
  checkHealth();
  setInterval(checkHealth, 30000);
  route();
});
