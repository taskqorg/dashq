// dashq.md D27: the browser adapter of /assetq/control. The server re-checks actor, grant and revision on every
// request; this file only carries a draft once and keeps an unknown operation for `Check delivery`.
'use strict';
const DashqAssetq = (() => {
  // Page-session state keyed by form id. A reload drops it; AssetQ and the journal keep the outcome (D22).
  const reviewed = new Map(), sent = new Map(), receipts = new Map();
  const tokens = new Map();
  const opId = () => [...crypto.getRandomValues(new Uint8Array(16))].map(b => b.toString(16).padStart(2, '0')).join('');
  const value = (form, name) => form.elements[name]?.value ?? '';

  function parseJSON(text, what) {
    try { const v = JSON.parse(text); if (v && typeof v === 'object' && !Array.isArray(v)) return v; } catch { /* named below */ }
    throw new Error(what + ' must be a JSON object');
  }

  function fields(form) {
    switch (form.dataset.assetqAction) {
      case 'job.submit': {
        const method = form.elements.method;
        return { method: method.value, operation: method.selectedOptions[0]?.dataset.operation ?? '', text: value(form, 'text'),
          attachments: value(form, 'attachments').split(/\s+/).filter(Boolean), constraints: parseJSON(value(form, 'constraints') || '{}', 'Constraints'),
          max_credits: Number(value(form, 'max_credits')) };
      }
      case 'job.cancel': return { job: value(form, 'job') };
      case 'method.save': return { id: value(form, 'id'), status: value(form, 'status'), spec: parseJSON(value(form, 'spec'), 'Specification') };
      case 'recommendation.promote': return { recommendation: value(form, 'recommendation'), use_case: value(form, 'use_case'), constraints: parseJSON(value(form, 'constraints'), 'Constraints') };
      default: throw new Error('Unknown action');
    }
  }

  function show(form) {
    const out = form.querySelector('.asset-receipt');
    const r = receipts.get(form.id);
    if (!out) return;
    out.replaceChildren();
    if (!r) return;
    out.append(r.text);
    if (r.state === 'unknown') {
      const check = document.createElement('button');
      check.type = 'button';
      check.textContent = 'Check delivery';
      check.addEventListener('click', () => deliver(form));
      out.append(' ', check);
    }
    form.querySelector('button[type=submit]').disabled = r.state === 'sending' || r.state === 'unknown';
  }

  async function csrf(project) {
    if (tokens.has(project)) return tokens.get(project);
    const r = await fetch('/assetq/control?project=' + encodeURIComponent(project), { cache: 'no-store' });
    const v = await r.json();
    if (!r.ok || !v.csrf) throw new Error(v.reason || 'unavailable');
    tokens.set(project, v.csrf);
    return v.csrf;
  }

  // One operation per draft. A lost reply keeps its ID and payload: a repeat reconciles, never starts a new one.
  async function deliver(form) {
    const op = sent.get(form.id);
    receipts.set(form.id, { state: 'sending', text: 'Sending' });
    show(form);
    let r, v;
    try {
      const token = await csrf(op.project);
      r = await fetch('/assetq/control', { method: 'POST', cache: 'no-store', headers: { 'Content-Type': 'application/json', 'X-Dashq-CSRF': token }, body: JSON.stringify(op) });
      v = await r.json();
    } catch {
      receipts.set(form.id, { state: 'unknown', text: 'Unknown · no reply. Check delivery repeats this operation; it never starts another.' });
      show(form);
      return;
    }
    const state = v.state ?? (r.ok ? 'unknown' : 'refused');
    const detail = [v.job && 'job ' + v.job, v.method, v.recommendation && 'recommendation ' + v.recommendation, v.effect, v.reason].filter(Boolean).join(' · ');
    receipts.set(form.id, { state, text: state[0].toUpperCase() + state.slice(1) + (detail ? ' · ' + detail : '') });
    if (state !== 'unknown') {
      sent.delete(form.id);
      reviewed.delete(form.id);
      if (state === 'accepted') refresh();
    }
    show(form);
  }

  function submit(event) {
    event.preventDefault();
    const form = event.currentTarget;
    if (sent.has(form.id)) return;
    let extra;
    try { extra = fields(form); } catch (e) {
      receipts.set(form.id, { state: 'refused', text: 'Refused · ' + e.message + '. Nothing was sent.' });
      show(form);
      return;
    }
    sent.set(form.id, { project: form.dataset.project, action: form.dataset.assetqAction, op_id: opId(), revision: reviewed.get(form.id) ?? form.dataset.revision, ...extra });
    deliver(form);
  }

  function bind() {
    document.querySelectorAll('#assets [data-assetq-action]').forEach(form => {
      const gate = form.querySelector('[data-assetq-can]');
      if (gate) gate.disabled = gate.dataset.assetqCan !== 'true';
      if (form.dataset.bound) return;
      form.dataset.bound = '1';
      form.addEventListener('submit', submit);
      // A draft keeps the revision it was started on, so a refresh cannot approve a change nobody reviewed.
      form.addEventListener('input', () => { if (!reviewed.has(form.id)) reviewed.set(form.id, form.dataset.revision); });
      show(form);
    });
  }
  return { bind };
})();
