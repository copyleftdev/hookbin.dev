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

async function renderRequestFeed(hookId) {
  show('view-requests');
  $('#view-requests').innerHTML = '<p class="empty-state">Loading requests...</p>';
}

async function renderRequestDetail(hookId, requestId) {
  show('view-detail');
  $('#view-detail').innerHTML = '<p class="empty-state">Loading request...</p>';
}

// --- Init ---
window.addEventListener('hashchange', route);
window.addEventListener('DOMContentLoaded', () => {
  checkHealth();
  setInterval(checkHealth, 30000);
  route();
});
