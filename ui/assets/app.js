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

// --- Views (stubs, implemented in HB-021 through HB-023) ---
async function renderHookList() {
  show('view-hooks');
  $('#view-hooks').innerHTML = '<p class="empty-state">Loading hooks...</p>';
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
