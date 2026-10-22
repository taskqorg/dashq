"use strict";
var TaskqAnswerCard = (() => {
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __commonJS = (cb, mod) => function __require() {
    try {
      return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
    } catch (e) {
      throw mod = 0, e;
    }
  };

  // node_modules/@taskq/coreq/src/telemetry/runtime.js
  var require_runtime = __commonJS({
    "node_modules/@taskq/coreq/src/telemetry/runtime.js"(exports, module) {
      (function(root) {
        "use strict";
        function create({ emit, id, clock = () => performance.now(), now = () => Date.now(), schedule = setTimeout, cancel = clearTimeout, autoFlush = false }) {
          const run = id(), metrics = /* @__PURE__ */ new Map();
          let current = null, serial = 0, lost = 0, diagnostic = null, pending = 0, flushTimer = null;
          if (!/^[a-f0-9]{32}$/.test(run)) throw Error("invalid telemetry run");
          const code = (name) => typeof name === "string" && name.length > 0 && name.length <= 128 && /^[a-zA-Z0-9._:-]+$/.test(name);
          const publish = (row) => {
            try {
              if (emit(row) !== false) return true;
            } catch (_) {
            }
            lost++;
            return false;
          };
          const scope = () => ({ trace: current?.trace || run + "." + ++serial, span: run + "." + ++serial, parent: current?.span || null });
          function invoke(name, fn, receiver, args, action2 = false) {
            const metric = metrics.get(name);
            if (metric) {
              metric.calls++;
              if (autoFlush && flushTimer === null) flushTimer = schedule(() => {
                flushTimer = null;
                flush();
              }, 2e3);
            }
            const selected = !action2 && diagnostic && diagnostic.remaining > 0 && (diagnostic.selector === "*" || name === diagnostic.selector || name.startsWith(diagnostic.selector + ".")) && clock() < diagnostic.until;
            if (selected) diagnostic.remaining--;
            const start = action2 || selected ? clock() : null, at = start === null ? null : now(), previous = current;
            const context = action2 || selected ? scope() : current;
            if (context) current = context;
            if (action2) publish({ kind: "ui.start", operation: name, ...context, started_ms: at, outcome: "unknown", error: "none" });
            let failed = false, unknown = false;
            try {
              const result = Reflect.apply(fn, receiver, args);
              unknown = result instanceof Promise;
              if (unknown) {
                if (metric) metric.unknown++;
                lost++;
              }
              return result;
            } catch (error) {
              failed = true;
              if (metric) metric.failed++;
              throw error;
            } finally {
              if (metric) metric.finished++;
              if (action2 || selected) publish({ kind: action2 ? "ui" : "method", source: action2 ? "action" : "diagnostic", operation: name, ...context, started_ms: at, duration_us: Math.max(0, Math.round((clock() - start) * 1e3)), outcome: failed ? "failed" : unknown ? "unknown" : "ok", error: failed ? "javascript-error" : unknown ? "unowned-async" : "none" });
              current = previous;
            }
          }
          function module2(name, methods) {
            if (!code(name)) throw Error("invalid telemetry module");
            const wrapped = {};
            for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(methods))) {
              if (typeof descriptor.value !== "function" || descriptor.value.constructor.name === "AsyncFunction") throw Error("module methods must be functions");
              const operation = "js." + name + "." + key;
              if (!code(operation)) throw Error("invalid telemetry method");
              if (!metrics.has(operation)) {
                if (metrics.size >= 256) throw Error("telemetry method limit");
                metrics.set(operation, { calls: 0, finished: 0, failed: 0, unknown: 0 });
              }
              Object.defineProperty(wrapped, key, { ...descriptor, value: function(...args) {
                return invoke(operation, descriptor.value, this, args);
              } });
            }
            return Object.freeze(wrapped);
          }
          function action(name, fn) {
            if (!code("js." + name)) throw Error("invalid telemetry action");
            return function(...args) {
              return invoke("js." + name, fn, this, args, true);
            };
          }
          function effect(name, start, receive) {
            if (!code("js." + name) || typeof start !== "function" || typeof receive !== "function" || receive.constructor.name === "AsyncFunction" || pending >= 64) throw Error("async effect unavailable");
            const previous = current, context = scope(), at = now(), began = clock();
            let waiting = true;
            pending++;
            publish({ kind: "effect.start", operation: "js." + name, ...context, started_ms: at, outcome: "unknown", error: "none" });
            const timer = schedule(() => {
              if (waiting) {
                publish({ kind: "effect.wait", operation: "js." + name, ...context, started_ms: at, outcome: "unknown", error: "completion-timeout" });
                deliver({ ok: false, unknown: true });
              }
            }, 3e4);
            let result;
            current = context;
            try {
              result = start();
            } catch (error) {
              settle(false, error);
              return;
            } finally {
              current = previous;
            }
            Promise.resolve(result).then((value) => settle(true, value), (error) => settle(false, error));
            function deliver(result2) {
              const outer = current;
              current = context;
              try {
                invoke("js." + name + ".result", receive, void 0, [result2], true);
              } catch (error) {
                if (typeof root.reportError === "function") root.reportError(error);
              } finally {
                current = outer;
              }
            }
            function settle(ok, value) {
              if (!waiting) return;
              waiting = false;
              pending--;
              cancel(timer);
              publish({ kind: "effect", operation: "js." + name, ...context, started_ms: at, duration_us: Math.max(0, Math.round((clock() - began) * 1e3)), outcome: ok ? "ok" : "failed", error: ok ? "none" : "javascript-error" });
              deliver({ ok, value });
            }
          }
          function bind(callback) {
            const context = current;
            return function(...args) {
              const previous = current;
              current = context;
              try {
                return Reflect.apply(callback, this, args);
              } finally {
                current = previous;
              }
            };
          }
          function diagnose(selector, milliseconds, limit) {
            if (selector !== "*" && !code(selector) || !Number.isInteger(milliseconds) || milliseconds < 1 || milliseconds > 6e4 || !Number.isInteger(limit) || limit < 1 || limit > 256) throw Error("invalid diagnostic selection");
            selector = selector === "*" ? selector : selector.replace(/\.+$/, "");
            if (!selector) throw Error("invalid diagnostic selection");
            diagnostic = { selector, until: clock() + milliseconds, remaining: limit };
            publish({ kind: "diagnostic", operation: "js.collector.diagnostic", source: selector, started_ms: now(), duration_us: milliseconds * 1e3, counts: { calls: limit }, outcome: "ok", error: "none" });
          }
          function flush() {
            if (flushTimer !== null) {
              cancel(flushTimer);
              flushTimer = null;
            }
            for (const [operation, counts] of metrics) {
              if (counts.calls && publish({ kind: "method.aggregate", operation, started_ms: now(), counts: { ...counts }, outcome: counts.failed ? "failed" : counts.unknown ? "unknown" : "ok", error: counts.failed ? "javascript-error" : counts.unknown ? "unowned-async" : "none" })) for (const key of Object.keys(counts)) counts[key] = 0;
            }
            if (autoFlush) publish({ kind: "collection", operation: "js.collector.health", started_ms: now(), counts: { lost, pending }, outcome: lost ? "unknown" : "ok", error: lost ? "collection-incomplete" : "none" });
          }
          return Object.freeze({ module: module2, action, effect, bind, diagnose, flush, context: () => current && { ...current }, health: () => ({ lost, pending, methods: metrics.size, diagnostic: diagnostic ? { selector: diagnostic.selector, remaining: diagnostic.remaining, active: clock() < diagnostic.until && diagnostic.remaining > 0 } : null }) });
        }
        function shared() {
          if (!root.__coreqExecution) {
            root.__coreqExecution = create({
              autoFlush: true,
              emit: (row) => root.__coreqExecutionEmit ? root.__coreqExecutionEmit(row) : false,
              id: () => Array.from(root.crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("")
            });
          }
          return root.__coreqExecution;
        }
        function connect(emit) {
          if (typeof emit !== "function") throw Error("invalid telemetry transport");
          root.__coreqExecutionEmit = emit;
          return shared();
        }
        const api = { create, shared, connect };
        if (typeof module === "object" && module.exports) module.exports = api;
        else root.CoreqExecution = api;
      })(typeof globalThis === "object" ? globalThis : exports);
    }
  });

  // src/ui-runtime.js
  var require_ui_runtime = __commonJS({
    "src/ui-runtime.js"(exports, module) {
      "use strict";
      var core = require_runtime();
      var execution = core.shared();
      var events = /* @__PURE__ */ new WeakMap();
      var frequent = /* @__PURE__ */ new Set(["scroll", "pointermove", "input"]);
      function send(message) {
        const context = execution.context();
        globalThis.ipc?.postMessage(JSON.stringify({ ...message, ...context ? { context: "1 " + context.trace + " " + context.span } : {} }));
      }
      function connect() {
        core.connect((row) => {
          if (!globalThis.ipc) return false;
          globalThis.ipc.postMessage(JSON.stringify({ a: "web-observation", row }));
          return true;
        });
      }
      function listen(target, type, callback, options) {
        const method = execution.module("ui.events", { [type]: callback })[type];
        const handler = execution.action("ui." + type, function(event) {
          if (event && typeof event === "object") events.set(event, execution.bind((fn) => fn()));
          return Reflect.apply(method, this, [event]);
        });
        const receive = function(event) {
          const inherited = event && events.get(event);
          return inherited ? inherited(() => Reflect.apply(method, this, [event])) : frequent.has(type) ? Reflect.apply(method, this, [event]) : Reflect.apply(handler, this, [event]);
        };
        target.addEventListener(type, receive, options);
        return () => target.removeEventListener(type, receive, options);
      }
      function defer(callback, delay = 0) {
        return setTimeout(execution.bind(execution.action("ui.timer", callback)), delay);
      }
      function repeat(callback, delay) {
        return setTimeout(callback, delay);
      }
      function cancel(timer) {
        clearTimeout(timer);
      }
      function effect(name, start, receive) {
        execution.effect(name, () => Promise.resolve().then(execution.bind(start)), (result) => receive(result.ok ? result.value : { state: "unknown", reason: result.unknown ? "Completion unknown" : String(result.value?.message || result.value) }));
      }
      module.exports = { execution, connect, send, listen, defer, repeat, cancel, effect };
    }
  });

  // src/answer-card.js
  var require_answer_card = __commonJS({
    "src/answer-card.js"(exports, module) {
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var { fresh, textProblem, status, step, render, receipt, mount, replay } = execution.module("answer-card", {
        fresh: function(row) {
          return { draft: "", bound: row.revision, op: null, receipt: null };
        },
        textProblem: function(q, text) {
          const lines = text.split("\n").filter((l) => l.trim());
          if (!lines.length) return "Type an answer";
          if (lines.length > q.limits.lines) return "At most " + q.limits.lines + " lines";
          if (lines.some((l) => [...l].length > q.limits.chars)) return "At most " + q.limits.chars + " characters per line";
          return null;
        },
        status: function(row, local, cap) {
          const q = row.question;
          if (local.op && local.receipt && (["sending", "unknown"].includes(local.receipt.state) || local.op.revision === row.revision || !q)) return local.receipt.state;
          if (!manageable(row, cap) && (!q || !q.parsed || !cap || !cap.answer || !["ask", "review"].includes(row.state))) return "read-only";
          if (local.bound !== row.revision && (local.draft.trim() || local.op || local.confirmClose)) return "stale";
          return "actionable";
        },
        step: function(row, local, cap, ev, newId) {
          const st = status(row, local, cap), l = { ...local };
          const { start } = execution.module("answer-card.step", { start: (payload) => {
            l.confirmClose = false;
            l.op = { id: newId(), task: row.n, revision: row.revision, ...payload };
            l.bound = row.revision;
            l.receipt = { state: "sending" };
            return { local: l, send: l.op };
          } });
          switch (ev.type) {
            case "input":
              if (st !== "stale") l.bound = row.revision;
              l.draft = ev.text;
              return { local: l, send: null };
            case "choose": {
              if (!["actionable", "rejected"].includes(st)) return { local, send: null };
              const c = row.question.choices.find((x) => x.id === ev.id);
              if (!c || c.consequential) return { local, send: null };
              return start({ choice: c.id });
            }
            case "submit":
              if (!cap || !cap.answer || !["actionable", "rejected"].includes(st) || textProblem(draftQuestion(row, cap), l.draft)) return { local, send: null };
              return start(manageable(row, cap) ? { action: "requeue", text: l.draft.trim() } : { text: l.draft.trim() });
            case "later":
            case "close":
            case "confirm-close":
              if (!manageable(row, cap) || !["actionable", "rejected"].includes(st)) return { local, send: null };
              if (ev.type === "close") return { local: { ...l, bound: row.revision, confirmClose: true }, send: null };
              if (ev.type === "confirm-close" && !l.confirmClose) return { local, send: null };
              return start(ev.type === "later" ? { action: "later" } : { action: "close", confirmed: true });
            case "cancel-close":
              return { local: { ...l, confirmClose: false }, send: null };
            case "review":
              if (st === "stale") {
                l.bound = row.revision;
                l.op = null;
                l.receipt = null;
                l.confirmClose = false;
              }
              return { local: l, send: null };
            case "receipt":
              if (!l.op || ev.opId !== l.op.id) return { local, send: null };
              l.receipt = { state: ["accepted", "rejected", "stale", "unknown"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason, next: ev.next };
              if (ev.state === "accepted" && l.op.text) l.draft = "";
              return { local: l, send: null };
            case "retry":
              return st === "unknown" ? { local: { ...l, receipt: { state: "sending" } }, send: l.op } : { local, send: null };
            default:
              return { local, send: null };
          }
        },
        render: function(board, row, local, cap) {
          const q = row.question || { text: row.card || "", parsed: false, choices: [], evidence: [] }, st = status(row, local, cap);
          const id = "qac-" + String(board).replace(/[^\w-]/g, "_") + "-" + row.n, live = st === "actionable" || st === "rejected";
          let h = '<section class="qac" data-qac-state="' + st + '" aria-labelledby="' + id + '-t">';
          h += '<p class="qac-head" id="' + id + '-t"><span class="qac-mark" aria-hidden="true">?</span> <a href="' + href(row.url) + '" target="_blank" rel="noopener">#' + esc(row.n) + "</a> " + esc(row.title) + "</p>";
          if (manageable(row, cap)) {
            h += '<pre class="qac-text">' + esc(q.text || row.card || "No question text supplied") + "</pre>";
            h += '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="1" aria-label="Requeue reason" placeholder="Reason to requeue"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(draftQuestion(row, cap), local.draft) ? "" : " disabled") + ">Requeue</button>" + hint(id, live && textProblem(draftQuestion(row, cap), local.draft)) + "</form>";
            h += '<div class="qac-choices"><button type="button" data-qac="later"' + (live ? "" : " disabled") + '>Later</button><button type="button" data-qac="close"' + (live ? "" : " disabled") + ">Close\u2026</button></div>";
            if (local.confirmClose && live) h += "<p>Cancel #" + esc(row.n) + ' without publication? Running work will stop. The task is parked before closing.</p><div class="qac-choices"><button type="button" data-qac="confirm-close">Close without publishing</button><button type="button" data-qac="cancel-close">Keep task</button></div>';
            h += receipt(st, local.receipt && local.receipt.reason, row.url, id, local.receipt && local.receipt.next, true);
            if (st === "stale") h += '<button type="button" data-qac="review">I read the new question</button>';
            if (st === "unknown") h += '<button type="button" data-qac="retry">Check delivery</button>';
            return h + "</section>";
          }
          if (!q.parsed) return h + '<pre class="qac-text">' + esc(q.text) + "</pre>" + receipt(st, st === "read-only" ? "This card has no answer form here" : local.receipt && local.receipt.reason, row.url, id) + (st === "unknown" ? '<button type="button" data-qac="retry">Check delivery</button>' : "") + "</section>";
          h += '<p class="qac-problem">' + linked(q.problem) + "</p>";
          h += '<div class="qac-choices" role="group" aria-label="Choices">';
          for (const c of q.choices) {
            const off = !live || c.consequential, why = c.consequential ? "Answer on the board: this choice closes or publishes the task" : "";
            h += '<button type="button" data-qac="choose" data-choice="' + c.id + '"' + (off ? " disabled" : "") + (why ? ' aria-describedby="' + id + "-c" + c.id + '"' : "") + '><span class="qac-number">' + c.id + ".</span> <span>" + esc(label(c.text)) + (c.recommended ? ' <span class="qac-star" role="img" aria-label="Recommended">\u2605</span>' : "") + "</span></button>";
            for (const m of c.text.matchAll(LINK)) h += '<a class="qac-link" href="' + href(m[2]) + '" target="_blank" rel="noopener">' + esc(m[1]) + " \u2197</a>";
            if (why) h += '<span class="qac-why" id="' + id + "-c" + c.id + '">' + esc(why) + ' <a href="' + href(row.url) + '" target="_blank" rel="noopener">board \u2197</a></span>';
          }
          h += "</div>";
          if (row.pick) h += '<p class="qac-pick"><span class="qac-muted">PM</span> ' + esc(row.n) + "." + esc(row.pick.id) + ": " + esc(row.pick.reason) + "</p>";
          if (st !== "read-only" || local.draft) {
            const bad = textProblem(q, local.draft);
            h += '<details class="qac-other"' + (local.draft ? " open" : "") + '><summary>Other answer</summary><form class="qac-text-form" data-qac="submit"><textarea id="' + id + '-d" data-qac="input" rows="2" aria-label="Your answer" placeholder="Or type an answer"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !bad ? "" : " disabled") + ">Send</button>" + hint(id, live && bad) + "</form></details>";
          }
          const reason = st === "read-only" ? cap && cap.reason || "Answering is not available here" : local.receipt && local.receipt.reason;
          h += receipt(st, reason, row.url, id, local.receipt && local.receipt.next);
          if (st === "stale") h += '<button type="button" data-qac="review">I read the new question</button>';
          if (st === "unknown") h += '<button type="button" data-qac="retry">Check delivery</button>';
          if (q.done || q.evidence.length) h += '<details class="qac-context"><summary>Checked &amp; context</summary>' + (q.done ? '<p class="qac-done"><span class="qac-muted">Done:</span> ' + esc(q.done) + "</p>" : "") + (q.evidence.length ? '<p class="qac-evidence">' + q.evidence.map((e) => '<a href="' + href(e.url) + '" target="_blank" rel="noopener">' + esc(e.label) + "</a>").join(" \xB7 ") + "</p>" : "") + "</details>";
          return h + "</section>";
        },
        receipt: function(st, reason, url, id, next, action) {
          if (st === "actionable") return '<p class="qac-receipt" id="' + id + '-r" role="status" aria-live="polite"></p>';
          return '<p class="qac-receipt qac-' + st + '" id="' + id + '-r" role="status" aria-live="polite">' + esc(st === "accepted" && action ? "Action accepted on the board" : LABELS[st]) + (reason ? ": " + esc(reason) : "") + (next ? ". " + esc(next) : "") + (st === "read-only" || st === "unknown" ? ' <a href="' + href(url) + '" target="_blank" rel="noopener">Open on board \u2197</a>' : "") + "</p>";
        },
        mount: function(el, adapter) {
          const { apply, paint, markup } = execution.module("answer-card.mount", {
            markup: function() {
              return [...cards].map(([k, c]) => '<div data-qac-key="' + esc(k) + '">' + render(c.board, c.row, locals.get(k), adapter.capability(c.board, c.row)) + "</div>").join("");
            },
            apply: function(k, ev) {
              const { board, row } = cards.get(k), cap = adapter.capability(board, row), r = step(row, locals.get(k), cap, ev, newId);
              locals.set(k, r.local);
              adapter.save?.(board, row, r.local);
              if (ev.type !== "input") paint();
              if (r.send) ui.effect("answer.send", () => adapter.send(board, r.send), (x) => {
                if (cards.has(k)) apply(k, { type: "receipt", opId: r.send.id, ...x });
              });
              else if (ev.type === "input") {
                const s = el.querySelector('[data-qac-key="' + CSS.escape(k) + '"] button[type=submit]'), cap2 = adapter.capability(board, row);
                const live = ["actionable", "rejected"].includes(status(row, r.local, cap2)), bad = textProblem(draftQuestion(row, cap2), r.local.draft);
                if (s) s.disabled = !!bad || !live;
                const h = s && s.parentNode.querySelector(".qac-hint");
                if (h) h.textContent = live && bad || "";
                painted = markup();
              }
            },
            paint: function() {
              const a = document.activeElement, focus = a && el.contains(a) ? { k: a.closest("[data-qac-key]")?.dataset.qacKey, sel: a.dataset.qac === "input" ? "textarea" : a.dataset.choice ? '[data-choice="' + a.dataset.choice + '"]' : '[data-qac="' + a.dataset.qac + '"]', s: a.selectionStart, e: a.selectionEnd } : null;
              const scroll = el.scrollTop, open = new Set([...el.querySelectorAll("details[open]")].map((d) => d.closest("[data-qac-key]")?.dataset.qacKey + " " + d.className));
              const html = markup();
              if (html === painted) return;
              el.innerHTML = painted = html;
              for (const d of el.querySelectorAll("details")) if (open.has(d.closest("[data-qac-key]").dataset.qacKey + " " + d.className)) d.open = true;
              el.scrollTop = scroll;
              for (const [k, local] of locals) if (Array.isArray(local.caret) && local.caret.length === 2) {
                const input = el.querySelector('[data-qac-key="' + CSS.escape(k) + '"] textarea');
                input?.setSelectionRange(...local.caret);
              }
              const t = focus && focus.k && el.querySelector('[data-qac-key="' + CSS.escape(focus.k) + '"] ' + focus.sel);
              if (t) {
                t.focus();
                if (focus.s != null && t.setSelectionRange) t.setSelectionRange(focus.s, focus.e);
              }
            }
          });
          const locals = /* @__PURE__ */ new Map(), cards = /* @__PURE__ */ new Map();
          let painted = null;
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "qac-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e) => {
            const b = e.target.closest("button[data-qac]"), k = b && b.closest("[data-qac-key]")?.dataset.qacKey;
            if (k && !b.disabled) apply(k, b.dataset.qac === "choose" ? { type: "choose", id: Number(b.dataset.choice) } : { type: b.dataset.qac });
          });
          ui.listen(el, "submit", (e) => {
            e.preventDefault();
            const k = e.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k) apply(k, { type: "submit" });
          });
          ui.listen(el, "keydown", (e) => {
            if (e.target.dataset.qac === "input" && e.key === "Enter" && !e.shiftKey && !e.isComposing && e.keyCode !== 229) {
              e.preventDefault();
              const k = e.target.closest("[data-qac-key]")?.dataset.qacKey;
              if (k) apply(k, { type: "submit" });
            }
          });
          for (const event of ["select", "keyup", "click"]) ui.listen(el, event, (e) => {
            const k = e.target.dataset.qac === "input" && e.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k && adapter.save) {
              const c = cards.get(k), local = { ...locals.get(k), caret: [e.target.selectionStart, e.target.selectionEnd] };
              locals.set(k, local);
              adapter.save(c.board, c.row, local);
            }
          });
          ui.listen(el, "input", (e) => {
            const k = e.target.dataset.qac === "input" && e.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k) apply(k, { type: "input", text: e.target.value });
          });
          return execution.module("answer-card.mount.controller", {
            receiveReceipt(board, opId, reply) {
              for (const [k, card] of cards) if (card.board === board && locals.get(k)?.op?.id === opId) {
                apply(k, { ...reply, type: "receipt", opId });
              }
            },
            // rows: [{board, row}] of open questions; a row that left keeps no state
            update(rows) {
              cards.clear();
              for (const c of rows) {
                const k = key(c.board, c.row);
                cards.set(k, c);
                if (!locals.has(k)) locals.set(k, adapter.load?.(c.board, c.row) || fresh(c.row));
              }
              for (const k of [...locals.keys()]) if (!cards.has(k)) locals.delete(k);
              paint();
            }
          });
        },
        replay: function(fx, capability) {
          let row = fx.rows[0], local = fresh(row), n = 0;
          const sent = [], out = [];
          for (const ev of fx.events || []) {
            if (ev.type === "refresh") {
              row = fx.rows[ev.row];
              continue;
            }
            if (ev.type === "check") {
              out.push({ status: status(row, local, capability(fx.board, row)), html: render(fx.board, row, local, capability(fx.board, row)), sent: sent.slice(), draft: local.draft });
              continue;
            }
            const r = step(row, local, capability(fx.board, row), ev.type === "receipt" ? { ...ev, opId: sent[sent.length - 1] && sent[sent.length - 1].id } : ev, () => "op-" + ++n);
            local = r.local;
            if (r.send) sent.push(r.send);
          }
          return out;
        }
      });
      var root = globalThis;
      var STATES = ["actionable", "sending", "accepted", "stale", "rejected", "unknown", "read-only"];
      var LABELS = {
        actionable: "Waiting for your answer",
        sending: "Sending\u2026",
        accepted: "Answer accepted on the board",
        stale: "The question changed: review it before sending",
        rejected: "Answer not accepted",
        unknown: "Delivery unknown: check the board",
        "read-only": "Read only"
      };
      var { esc } = execution.module("answer-card", { esc: (s) => String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]) });
      var { key } = execution.module("answer-card", { key: (board, row) => board + "#" + row.n });
      var { href } = execution.module("answer-card", { href: (u) => /^https?:\/\//i.test(u || "") ? esc(u) : "#" });
      var LINK = /\[([^\]]*)\]\(([^)\s]*)\)/g;
      var { label } = execution.module("answer-card", { label: (s) => String(s == null ? "" : s).replace(LINK, "$1") });
      var { linked } = execution.module("answer-card", { linked: (s) => {
        const p = String(s == null ? "" : s).split(LINK);
        let h = esc(p[0]);
        for (let i = 1; i < p.length; i += 3) h += '<a href="' + href(p[i + 1]) + '" target="_blank" rel="noopener">' + esc(p[i]) + "</a>" + esc(p[i + 2]);
        return h;
      } });
      var { manageable } = execution.module("answer-card", { manageable: (row, cap) => row.state === "ask" && cap && cap.answer && cap.manage && !(row.question && row.question.choices && row.question.choices.length) });
      var { hint } = execution.module("answer-card", { hint: (id, why) => '<span class="qac-hint" id="' + id + '-h" aria-live="polite">' + esc(why || "") + "</span>" });
      var { draftQuestion } = execution.module("answer-card", { draftQuestion: (row, cap) => manageable(row, cap) ? { limits: { lines: 1, chars: 232 } } : row.question });
      var api = { STATES, status, step, render, mount, replay, textProblem };
      module.exports = api;
    }
  });
  return require_answer_card();
})();
