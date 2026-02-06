// hookbin dashboard
'use strict';

// --- API Client ---
const api = {
  async get(path) {
    const res = await fetch(path);
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw { status: res.status, ...err };
    }
    return res.json();
  },

  async post(path, body) {
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
  },

  async del(path) {
    const res = await fetch(path, { method: 'DELETE' });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw { status: res.status, ...err };
    }
    return res.json();
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
  setTimeout(() => el.remove(), 2500);
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

// --- Router ---
function getRoute() {
  const hash = location.hash.slice(1) || '/hooks';
  return hash;
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
async function renderHookList() {
  show('view-hooks');
  const el = $('#view-hooks');
  el.innerHTML = '<p class="empty-state">Loading hooks...</p>';

  const data = await api.get('/api/hooks');
  const hooks = data.hooks || [];
  const origin = location.origin;

  let html = '<div class="toolbar">';
  html += '<h2 style="margin:0;font-size:16px;">Hooks (' + data.count + ')</h2>';
  html += '<button class="btn btn-primary" id="btn-create-hook">+ Create Hook</button>';
  html += '</div>';

  if (hooks.length === 0) {
    html += '<p class="empty-state">No hooks yet. Create one to start capturing webhooks.</p>';
  } else {
    for (const hook of hooks) {
      const url = origin + hook.url;
      html += '<div class="card card-clickable" data-hook-id="' + escapeHtml(hook.hook_id) + '">';
      html += '<div class="card-header">';
      html += '<span class="card-title">' + escapeHtml(hook.name) + '</span>';
      html += '<span style="display:flex;gap:6px;flex-shrink:0;">';
      html += '<button class="btn btn-small btn-copy-url" data-url="' + escapeHtml(url) + '" title="Copy URL">copy</button>';
      html += '<button class="btn btn-small btn-danger btn-delete-hook" data-hook-id="' + escapeHtml(hook.hook_id) + '" data-hook-name="' + escapeHtml(hook.name) + '" title="Delete">del</button>';
      html += '</span>';
      html += '</div>';
      html += '<div class="card-meta">';
      html += '<span>' + hook.request_count + ' request' + (hook.request_count !== 1 ? 's' : '') + '</span>';
      html += '<span class="url-box" style="border:0;padding:0;background:none;font-size:12px;">' + escapeHtml(url) + '</span>';
      html += '<span>' + timeAgo(hook.created_at) + '</span>';
      html += '</div>';
      html += '</div>';
    }
  }

  el.innerHTML = html;

  // Create hook handler
  const createBtn = document.getElementById('btn-create-hook');
  if (createBtn) {
    createBtn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const name = prompt('Hook name (leave blank for auto-generated):');
      if (name === null) return;
      try {
        const body = name.trim() ? { name: name.trim() } : {};
        await api.post('/api/hooks', body);
        await renderHookList();
      } catch (err) {
        toast(err.error || 'Failed to create hook', 'error');
      }
    });
  }

  // Copy URL handlers
  el.querySelectorAll('.btn-copy-url').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      copyText(btn.dataset.url);
    });
  });

  // Delete hook handlers
  el.querySelectorAll('.btn-delete-hook').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const name = btn.dataset.hookName;
      if (!confirm('Delete hook "' + name + '"? All captured requests will be lost.')) return;
      try {
        await api.del('/api/hooks/' + btn.dataset.hookId);
        toast('Hook deleted', 'success');
        await renderHookList();
      } catch (err) {
        toast(err.error || 'Failed to delete hook', 'error');
      }
    });
  });

  // Click card to navigate
  el.querySelectorAll('.card-clickable').forEach(card => {
    card.addEventListener('click', () => {
      navigate('/hooks/' + card.dataset.hookId);
    });
  });
}

// --- Request Feed View ---
async function renderRequestFeed(hookId) {
  show('view-requests');
  const el = $('#view-requests');
  el.innerHTML = '<p class="empty-state">Loading requests...</p>';

  const hook = await api.get('/api/hooks/' + hookId);
  const origin = location.origin;
  const ingestUrl = origin + hook.url;

  let offset = 0;
  const limit = 50;

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

    const currentCount = listEl.querySelectorAll('.card').length + requests.length;
    statusEl.textContent = 'Showing ' + currentCount + ' of ' + total;

    if (total === 0 && !append) {
      listEl.innerHTML = '<p class="empty-state">No requests captured yet. Send a webhook to ' + escapeHtml(ingestUrl) + '</p>';
      footerEl.innerHTML = '';
      return;
    }

    let cards = '';
    for (const req of requests) {
      cards += '<div class="card card-clickable" data-hook-id="' + escapeHtml(hookId) + '" data-request-id="' + escapeHtml(req.request_id) + '">';
      cards += '<div class="card-header">';
      cards += '<span>' + methodBadge(req.method) + ' <span style="color:var(--text-muted);">' + escapeHtml(req.path) + '</span></span>';
      cards += '<span class="card-meta" style="margin:0;">' + timeAgo(req.received_at) + '</span>';
      cards += '</div>';
      cards += '<div class="card-meta">';
      cards += '<span>' + formatBytes(req.content_length) + '</span>';
      cards += '<span>' + escapeHtml(req.source_ip) + '</span>';
      cards += '</div>';
      cards += '</div>';
    }

    if (append) {
      listEl.insertAdjacentHTML('beforeend', cards);
    } else {
      listEl.innerHTML = cards;
    }

    // Click to view detail
    listEl.querySelectorAll('.card-clickable').forEach(card => {
      card.onclick = () => {
        navigate('/hooks/' + card.dataset.hookId + '/requests/' + card.dataset.requestId);
      };
    });

    // Load more
    if (currentCount < total) {
      footerEl.innerHTML = '<button class="btn" id="btn-load-more">Load more</button>';
      document.getElementById('btn-load-more').addEventListener('click', async () => {
        offset += limit;
        try {
          await loadRequests(true);
        } catch (err) {
          toast(err.error || 'Failed to load more', 'error');
        }
      });
    } else {
      footerEl.innerHTML = '';
    }
  }

  await loadRequests(false);
}

// --- Request Inspector View ---
async function renderRequestDetail(hookId, requestId) {
  show('view-detail');
  const el = $('#view-detail');
  el.innerHTML = '<p class="empty-state">Loading request...</p>';

  const req = await api.get('/api/hooks/' + hookId + '/requests/' + requestId);

  // Decode body from base64
  let bodyText = '';
  let bodyDisplay = '';
  let isBinary = false;
  if (!req.body || req.body === '') {
    bodyDisplay = '<span style="color:var(--text-muted);">(empty body)</span>';
  } else {
    try {
      const raw = atob(req.body);
      // Check if it's valid UTF-8 text
      const bytes = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
      bodyText = new TextDecoder('utf-8', { fatal: true }).decode(bytes);

      // Try to pretty-print JSON
      try {
        const parsed = JSON.parse(bodyText);
        bodyDisplay = '<pre class="code-block">' + escapeHtml(JSON.stringify(parsed, null, 2)) + '</pre>';
      } catch {
        bodyDisplay = '<pre class="code-block">' + escapeHtml(bodyText) + '</pre>';
      }
    } catch {
      isBinary = true;
      bodyDisplay = '<span style="color:var(--text-muted);">(binary payload, ' + formatBytes(req.content_length) + ')</span>';
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

  // Summary
  html += '<div class="card" style="margin-bottom:16px;">';
  html += '<div class="card-header">';
  html += '<span>' + methodBadge(req.method) + ' <span style="color:var(--text-muted);">' + escapeHtml(req.path) + '</span></span>';
  html += '</div>';
  html += '<div class="card-meta">';
  html += '<span>' + formatBytes(req.content_length) + '</span>';
  html += '<span>' + escapeHtml(req.source_ip) + '</span>';
  html += '<span>' + escapeHtml(timestamp) + '</span>';
  html += '<span>' + timeAgo(req.received_at) + '</span>';
  html += '</div>';
  html += '</div>';

  // Headers
  const headers = req.headers || {};
  const headerKeys = Object.keys(headers);
  html += '<div class="section-header">';
  html += '<span class="section-title">Headers (' + headerKeys.length + ')</span>';
  if (headerKeys.length > 0) {
    html += '<button class="btn btn-small" id="btn-copy-headers">copy</button>';
  }
  html += '</div>';

  if (headerKeys.length > 0) {
    html += '<table class="kv-table">';
    for (const key of headerKeys.sort()) {
      html += '<tr><th>' + escapeHtml(key) + '</th><td>' + escapeHtml(headers[key]) + '</td></tr>';
    }
    html += '</table>';
  } else {
    html += '<p style="color:var(--text-muted);font-size:13px;">(no headers)</p>';
  }

  // Body
  html += '<div class="section-header" style="margin-top:20px;">';
  html += '<span class="section-title">Body</span>';
  if (bodyText && !isBinary) {
    html += '<button class="btn btn-small" id="btn-copy-body">copy</button>';
  }
  html += '</div>';
  html += bodyDisplay;

  el.innerHTML = html;

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
      copyText(bodyText);
    });
  }
}

// --- Init ---
window.addEventListener('hashchange', route);
window.addEventListener('DOMContentLoaded', () => {
  checkHealth();
  setInterval(checkHealth, 30000);
  route();
});
