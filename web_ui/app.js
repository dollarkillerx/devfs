// devfs Web Management UI
(function () {
  'use strict';

  const API = '/_web/api';
  let currentBucket = null;
  let currentPrefix = '';

  // ── Helpers ────────────────────────────────────────────────────────────

  function getToken() {
    return localStorage.getItem('devfs_token');
  }

  async function api(path, opts = {}) {
    const token = getToken();
    const headers = { 'Content-Type': 'application/json', ...opts.headers };
    if (token) headers['Authorization'] = 'Bearer ' + token;
    const res = await fetch(API + path, { ...opts, headers });
    if (res.status === 401) {
      localStorage.removeItem('devfs_token');
      showLogin();
      throw new Error('Unauthorized');
    }
    return res;
  }

  function $(sel) { return document.querySelector(sel); }
  function $$(sel) { return document.querySelectorAll(sel); }

  function formatSize(bytes) {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
  }

  function formatDate(iso) {
    if (!iso) return '';
    const d = new Date(iso);
    return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  // ── SVG Icons ──────────────────────────────────────────────────────────

  const ICON_FOLDER = '<svg class="folder-icon" viewBox="0 0 20 20" fill="currentColor"><path d="M2 6a2 2 0 012-2h4l2 2h6a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/></svg>';

  const ICON_EMPTY_BUCKET = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4"/></svg>';

  const ICON_EMPTY_FOLDER = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>';

  const ICON_EMPTY_KEY = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/></svg>';

  // ── Toast ──────────────────────────────────────────────────────────────

  let toastTimer = null;
  let toastHideTimer = null;

  function showToast(msg, type) {
    const el = document.getElementById('toast');
    clearTimeout(toastTimer);
    clearTimeout(toastHideTimer);

    // Reset animation
    el.classList.remove('is-hiding');
    el.hidden = true;
    void el.offsetHeight; // reflow

    el.textContent = msg;
    el.className = 'toast ' + (type || 'success');
    el.hidden = false;

    toastTimer = setTimeout(() => {
      el.classList.add('is-hiding');
      toastHideTimer = setTimeout(() => { el.hidden = true; }, 300);
    }, 2500);
  }

  // ── Modal helpers ──────────────────────────────────────────────────────

  function openModal(sel) {
    $(sel).classList.add('is-visible');
  }

  function closeModal(sel) {
    $(sel).classList.remove('is-visible');
  }

  // Click-outside-to-close for modals
  document.addEventListener('click', (e) => {
    if (e.target.classList.contains('modal-overlay') && e.target.classList.contains('is-visible')) {
      e.target.classList.remove('is-visible');
    }
  });

  // ── Pages ──────────────────────────────────────────────────────────────

  function showLogin() {
    $('#login-page').hidden = false;
    $('#dashboard-page').hidden = true;
  }

  function showDashboard() {
    $('#login-page').hidden = true;
    $('#dashboard-page').hidden = false;
    switchSection('buckets');
  }

  function switchSection(name) {
    $$('.nav-links a').forEach(a => a.classList.toggle('active', a.dataset.page === name));
    $('#buckets-section').hidden = name !== 'buckets';
    $('#bucket-detail-section').hidden = true;
    $('#keys-section').hidden = name !== 'keys';

    if (name === 'buckets') loadBuckets();
    if (name === 'keys') loadKeys();
  }

  // ── Login ──────────────────────────────────────────────────────────────

  $('#login-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const user = $('#login-user').value;
    const pass = $('#login-pass').value;
    $('#login-error').hidden = true;

    try {
      const res = await fetch(API + '/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username: user, password: pass }),
      });
      const data = await res.json();
      if (!res.ok) {
        $('#login-error').textContent = data.error || 'Login failed';
        $('#login-error').hidden = false;
        return;
      }
      localStorage.setItem('devfs_token', data.token);
      showDashboard();
    } catch (err) {
      $('#login-error').textContent = 'Connection error';
      $('#login-error').hidden = false;
    }
  });

  $('#logout-btn').addEventListener('click', async () => {
    await api('/logout', { method: 'POST' }).catch(() => {});
    localStorage.removeItem('devfs_token');
    showLogin();
  });

  // ── Navigation ─────────────────────────────────────────────────────────

  $$('.nav-links a').forEach(a => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      switchSection(a.dataset.page);
    });
  });

  // ── Buckets ────────────────────────────────────────────────────────────

  async function loadBuckets() {
    try {
      const res = await api('/buckets');
      const data = await res.json();
      const tbody = $('#buckets-table tbody');
      tbody.innerHTML = '';
      const buckets = data.buckets || [];

      if (buckets.length === 0) {
        const tr = document.createElement('tr');
        tr.innerHTML = `<td colspan="5"><div class="empty-state">${ICON_EMPTY_BUCKET}<div class="empty-message">No buckets yet</div><div class="empty-hint">Create your first bucket to get started</div></div></td>`;
        tbody.appendChild(tr);
        return;
      }

      buckets.forEach(b => {
        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td><a href="#" class="folder-link" data-bucket="${esc(b.name)}">${esc(b.name)}</a></td>
          <td>${formatDate(b.created)}</td>
          <td>${b.policy.public_read ? 'Yes' : 'No'}</td>
          <td>${b.policy.public_write ? 'Yes' : 'No'}</td>
          <td><button class="btn-small btn-danger" data-delete-bucket="${esc(b.name)}">Delete</button></td>
        `;
        tbody.appendChild(tr);
      });

      // Bind events
      tbody.querySelectorAll('[data-bucket]').forEach(a => {
        a.addEventListener('click', (e) => {
          e.preventDefault();
          openBucket(a.dataset.bucket);
        });
      });
      tbody.querySelectorAll('[data-delete-bucket]').forEach(btn => {
        btn.addEventListener('click', () => deleteBucket(btn.dataset.deleteBucket));
      });
    } catch (e) { /* handled by api() */ }
  }

  $('#create-bucket-btn').addEventListener('click', async () => {
    const name = $('#new-bucket-name').value.trim();
    if (!name) { showToast('Please enter a bucket name', 'error'); return; }
    try {
      const res = await api('/buckets', {
        method: 'POST',
        body: JSON.stringify({ name }),
      });
      if (res.ok) {
        $('#new-bucket-name').value = '';
        showToast('Bucket created', 'success');
        loadBuckets();
      } else {
        const data = await res.json();
        showToast(data.error || 'Failed to create bucket', 'error');
      }
    } catch (e) { /* handled */ }
  });

  async function deleteBucket(name) {
    if (!confirm(`Delete bucket "${name}"? This cannot be undone.`)) return;
    try {
      const res = await api(`/buckets/${encodeURIComponent(name)}`, { method: 'DELETE' });
      if (res.ok) { showToast('Bucket deleted', 'success'); loadBuckets(); }
      else {
        const data = await res.json();
        showToast(data.error || 'Failed to delete bucket', 'error');
      }
    } catch (e) { /* handled */ }
  }

  // ── Bucket Detail ──────────────────────────────────────────────────────

  async function openBucket(name) {
    currentBucket = name;
    currentPrefix = '';
    $('#buckets-section').hidden = true;
    $('#bucket-detail-section').hidden = false;
    $('#bucket-detail-name').textContent = name;
    await loadBucketDetail();
  }

  $('#back-to-buckets').addEventListener('click', () => {
    $('#bucket-detail-section').hidden = true;
    $('#buckets-section').hidden = false;
    loadBuckets();
  });

  async function loadBucketDetail() {
    // Load policy
    try {
      const res = await api('/buckets');
      const data = await res.json();
      const bucket = (data.buckets || []).find(b => b.name === currentBucket);
      if (bucket) {
        $('#policy-public-read').checked = bucket.policy.public_read;
        $('#policy-public-write').checked = bucket.policy.public_write;
      }
    } catch (e) { /* handled */ }

    await loadObjects();
  }

  $('#save-policy-btn').addEventListener('click', async () => {
    try {
      const res = await api(`/buckets/${encodeURIComponent(currentBucket)}/policy`, {
        method: 'PUT',
        body: JSON.stringify({
          public_read: $('#policy-public-read').checked,
          public_write: $('#policy-public-write').checked,
        }),
      });
      if (res.ok) showToast('Policy saved', 'success');
      else showToast('Failed to save policy', 'error');
    } catch (e) { showToast('Failed to save policy', 'error'); }
  });

  async function loadObjects() {
    updateBreadcrumb();
    try {
      let url = `/buckets/${encodeURIComponent(currentBucket)}/objects?delimiter=/`;
      if (currentPrefix) url += `&prefix=${encodeURIComponent(currentPrefix)}`;

      const res = await api(url);
      const data = await res.json();
      const tbody = $('#objects-table tbody');
      tbody.innerHTML = '';

      const prefixes = data.common_prefixes || [];
      const objects = data.objects || [];

      if (prefixes.length === 0 && objects.length === 0) {
        const tr = document.createElement('tr');
        tr.innerHTML = `<td colspan="5"><div class="empty-state">${ICON_EMPTY_FOLDER}<div class="empty-message">This folder is empty</div><div class="empty-hint">Upload files or create folders to get started</div></div></td>`;
        tbody.appendChild(tr);
        return;
      }

      // Folders (common prefixes)
      prefixes.forEach(prefix => {
        const display = prefix.replace(currentPrefix, '');
        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td><span class="folder-link" data-prefix="${esc(prefix)}">${ICON_FOLDER} ${esc(display)}</span></td>
          <td class="text-muted">-</td>
          <td class="text-muted">-</td>
          <td class="text-muted col-etag">-</td>
          <td></td>
        `;
        tbody.appendChild(tr);
      });

      // Files
      objects.forEach(obj => {
        const display = obj.key.replace(currentPrefix, '');
        if (!display) return; // skip prefix itself
        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td>${esc(display)}</td>
          <td>${formatSize(obj.size)}</td>
          <td>${formatDate(obj.last_modified)}</td>
          <td class="col-etag"><code>${esc(obj.etag)}</code></td>
          <td>
            <button class="btn-text btn-small" data-download-obj="${esc(obj.key)}">Download</button>
            <button class="btn-small btn-danger" data-delete-obj="${esc(obj.key)}">Delete</button>
          </td>
        `;
        tbody.appendChild(tr);
      });

      // Bind events
      tbody.querySelectorAll('[data-prefix]').forEach(el => {
        el.addEventListener('click', () => {
          currentPrefix = el.dataset.prefix;
          loadObjects();
        });
      });
      tbody.querySelectorAll('[data-download-obj]').forEach(btn => {
        btn.addEventListener('click', () => downloadObject(btn.dataset.downloadObj));
      });
      tbody.querySelectorAll('[data-delete-obj]').forEach(btn => {
        btn.addEventListener('click', () => deleteObject(btn.dataset.deleteObj));
      });
    } catch (e) { /* handled */ }
  }

  function updateBreadcrumb() {
    const bc = $('#breadcrumb');
    bc.innerHTML = '';
    const root = document.createElement('a');
    root.href = '#';
    root.textContent = currentBucket;
    root.addEventListener('click', (e) => { e.preventDefault(); currentPrefix = ''; loadObjects(); });
    bc.appendChild(root);

    if (currentPrefix) {
      const parts = currentPrefix.split('/').filter(Boolean);
      let path = '';
      parts.forEach((part, i) => {
        path += part + '/';
        const sep = document.createElement('span');
        sep.textContent = ' / ';
        bc.appendChild(sep);

        const a = document.createElement('a');
        a.href = '#';
        a.textContent = part;
        const p = path; // capture
        a.addEventListener('click', (e) => { e.preventDefault(); currentPrefix = p; loadObjects(); });
        bc.appendChild(a);
      });
    }
  }

  // ── Upload ─────────────────────────────────────────────────────────────

  $('#upload-btn').addEventListener('click', async () => {
    const files = $('#upload-file').files;
    if (!files.length) return;

    for (const file of files) {
      const form = new FormData();
      form.append('prefix', currentPrefix);
      form.append('file', file);

      try {
        const headers = {};
        const token = getToken();
        if (token) headers['Authorization'] = 'Bearer ' + token;
        const res = await fetch(`${API}/buckets/${encodeURIComponent(currentBucket)}/upload`, {
          method: 'POST',
          headers,
          body: form,
        });
        if (res.status === 401) {
          localStorage.removeItem('devfs_token');
          showLogin();
          return;
        }
      } catch (e) { /* handled */ }
    }

    $('#upload-file').value = '';
    loadObjects();
  });

  async function downloadObject(key) {
    try {
      const res = await api(`/buckets/${encodeURIComponent(currentBucket)}/download/${encodeURIComponent(key)}`);
      if (!res.ok) return;
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = key.split('/').pop() || key;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (e) { /* handled by api() */ }
  }

  async function deleteObject(key) {
    if (!confirm(`Delete "${key}"?`)) return;
    try {
      const res = await api(`/buckets/${encodeURIComponent(currentBucket)}/objects/${encodeURIComponent(key)}`, {
        method: 'DELETE',
      });
      if (res.ok) { showToast('Object deleted', 'success'); loadObjects(); }
      else {
        const data = await res.json();
        showToast(data.error || 'Failed to delete object', 'error');
      }
    } catch (e) { /* handled */ }
  }

  // ── API Keys ───────────────────────────────────────────────────────────

  let allBuckets = [];

  async function loadKeys() {
    // Also load bucket list for permissions
    try {
      const bRes = await api('/buckets');
      const bData = await bRes.json();
      allBuckets = (bData.buckets || []).map(b => b.name);
    } catch (e) { allBuckets = []; }

    try {
      const res = await api('/keys');
      const data = await res.json();
      const tbody = $('#keys-table tbody');
      tbody.innerHTML = '';
      const keys = data.keys || [];

      if (keys.length === 0) {
        const tr = document.createElement('tr');
        tr.innerHTML = `<td colspan="5"><div class="empty-state">${ICON_EMPTY_KEY}<div class="empty-message">No API keys yet</div><div class="empty-hint">Create an API key for programmatic access</div></div></td>`;
        tbody.appendChild(tr);
        return;
      }

      keys.forEach(k => {
        const perms = Object.entries(k.buckets || {})
          .map(([b, p]) => `${b}: ${p}`)
          .join(', ') || 'none';

        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td><code>${esc(k.access_key)}</code></td>
          <td>${esc(k.description)}</td>
          <td>${formatDate(k.created_at)}</td>
          <td class="col-perms">
            <span class="text-muted">${esc(perms)}</span>
            <button class="btn-text btn-small" data-edit-key="${esc(k.id)}">Edit</button>
          </td>
          <td><button class="btn-small btn-danger" data-delete-key="${esc(k.id)}">Delete</button></td>
        `;
        tbody.appendChild(tr);
      });

      tbody.querySelectorAll('[data-edit-key]').forEach(btn => {
        btn.addEventListener('click', () => {
          const key = keys.find(k => k.id === btn.dataset.editKey);
          if (key) openPermModal(key);
        });
      });
      tbody.querySelectorAll('[data-delete-key]').forEach(btn => {
        btn.addEventListener('click', () => deleteKey(btn.dataset.deleteKey));
      });
    } catch (e) { /* handled */ }
  }

  // ── Create Key Modal ───────────────────────────────────────────────────

  $('#open-create-key-btn').addEventListener('click', async () => {
    // Ensure bucket list is fresh
    try {
      const bRes = await api('/buckets');
      const bData = await bRes.json();
      allBuckets = (bData.buckets || []).map(b => b.name);
    } catch (e) { /* use existing allBuckets */ }

    $('#create-key-desc').value = '';
    const list = $('#create-key-bucket-list');
    list.innerHTML = '';
    allBuckets.forEach(bucket => {
      const row = document.createElement('div');
      row.className = 'perm-row';
      row.innerHTML = `
        <label>${esc(bucket)}</label>
        <select data-bucket="${esc(bucket)}">
          <option value="none" selected>None</option>
          <option value="read">Read</option>
          <option value="read_write">Read/Write</option>
        </select>
      `;
      list.appendChild(row);
    });
    openModal('#create-key-modal');
  });

  $('#create-key-cancel-btn').addEventListener('click', () => {
    closeModal('#create-key-modal');
  });

  $('#create-key-confirm-btn').addEventListener('click', async () => {
    const desc = $('#create-key-desc').value.trim();
    const buckets = {};
    $('#create-key-bucket-list').querySelectorAll('select').forEach(sel => {
      buckets[sel.dataset.bucket] = sel.value;
    });

    try {
      const res = await api('/keys', {
        method: 'POST',
        body: JSON.stringify({ description: desc, buckets }),
      });
      if (res.ok) {
        const data = await res.json();
        closeModal('#create-key-modal');
        // Show secret banner
        $('#new-ak').textContent = data.access_key;
        $('#new-sk').textContent = data.secret_key;
        $('#new-key-banner').hidden = false;
        showToast('API key created', 'success');
        loadKeys();
      } else {
        const data = await res.json();
        showToast(data.error || 'Failed to create key', 'error');
      }
    } catch (e) { /* handled */ }
  });

  $('#dismiss-key-banner').addEventListener('click', () => {
    $('#new-key-banner').hidden = true;
  });

  async function deleteKey(id) {
    if (!confirm('Delete this API key? This cannot be undone.')) return;
    try {
      const res = await api(`/keys/${encodeURIComponent(id)}`, { method: 'DELETE' });
      if (res.ok) loadKeys();
    } catch (e) { /* handled */ }
  }

  // ── Permission Modal ───────────────────────────────────────────────────

  let editingKeyId = null;

  function openPermModal(key) {
    editingKeyId = key.id;
    $('#perm-modal-key-info').textContent = `Key: ${key.access_key}`;
    $('#perm-modal-desc').value = key.description || '';
    const list = $('#perm-bucket-list');
    list.innerHTML = '';

    allBuckets.forEach(bucket => {
      const current = (key.buckets || {})[bucket] || 'none';
      const row = document.createElement('div');
      row.className = 'perm-row';
      row.innerHTML = `
        <label>${esc(bucket)}</label>
        <select data-bucket="${esc(bucket)}">
          <option value="none" ${current === 'none' ? 'selected' : ''}>None</option>
          <option value="read" ${current === 'read' ? 'selected' : ''}>Read</option>
          <option value="read_write" ${current === 'read_write' ? 'selected' : ''}>Read/Write</option>
        </select>
      `;
      list.appendChild(row);
    });

    openModal('#perm-modal');
  }

  $('#perm-cancel-btn').addEventListener('click', () => {
    closeModal('#perm-modal');
  });

  $('#perm-save-btn').addEventListener('click', async () => {
    const buckets = {};
    $('#perm-bucket-list').querySelectorAll('select').forEach(sel => {
      buckets[sel.dataset.bucket] = sel.value;
    });
    const description = $('#perm-modal-desc').value.trim();

    try {
      const res = await api(`/keys/${encodeURIComponent(editingKeyId)}`, {
        method: 'PUT',
        body: JSON.stringify({ buckets, description }),
      });
      if (res.ok) {
        showToast('Key updated', 'success');
      } else {
        const data = await res.json();
        showToast(data.error || 'Failed to update key', 'error');
      }
      closeModal('#perm-modal');
      loadKeys();
    } catch (e) { /* handled */ }
  });

  // ── Utility ────────────────────────────────────────────────────────────

  function esc(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
  }

  // ── Init ───────────────────────────────────────────────────────────────

  // Check localStorage token and validate against server
  (async function init() {
    if (!getToken()) {
      showLogin();
      return;
    }
    try {
      const res = await api('/buckets');
      if (res.ok) {
        showDashboard();
      } else {
        showLogin();
      }
    } catch (e) {
      showLogin();
    }
  })();
})();
