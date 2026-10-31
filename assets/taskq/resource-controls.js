"use strict";
var TaskqResources = (() => {
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
      var shared = core.shared();
      var events = /* @__PURE__ */ new WeakMap();
      var execution = { ...shared, module(name, methods) {
        const wrapped = {};
        for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(methods))) {
          let method;
          Object.defineProperty(wrapped, key, { ...descriptor, value: function(...args) {
            method ??= shared.module(name, { [key]: descriptor.value })[key];
            return Reflect.apply(method, this, args);
          } });
        }
        return Object.freeze(wrapped);
      } };
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

  // src/resource-controls.js
  var require_resource_controls = __commonJS({
    "src/resource-controls.js"(exports, module) {
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var { scopes, freshness, policy, gate, age, usage, applied, headline, fields, edited, problem, changes, review, fresh, status, step, render, input, mount, workFields, freshWork, workStatus, workStep, workRender, mountWork, replayWork, replay } = execution.module("resource-controls", {
        scopes: function(group) {
          const t = group.targets || [];
          const host = t.find((x) => x.target.project_id == null && x.target.runtime_id == null), project = t.find((x) => x.target.project_id != null && x.target.runtime_id == null);
          return { host, project, observation: schemaOf(host, "taskq.host-capacity/1") ? host : project, runtimes: t.filter((x) => x.target.runtime_id != null) };
        },
        freshness: function(group, now) {
          const p = scopes(group).observation, obs = group.observed_at, s = p && p.sample;
          const status2 = obs == null || now < obs ? null : now - obs;
          const sample = s && s.sampled_at != null && s.age_seconds != null && status2 != null && s.sampled_at <= obs ? s.age_seconds + status2 : null;
          return { status: status2, sample, fresh: status2 != null && status2 <= FRESH };
        },
        policy: function(group) {
          const h = scopes(group).host, c = h && h.current_policy || {};
          if (schemaOf(h, "taskq.host-capacity/1")) return "Host limits \xB7 " + (c.maximum == null ? "no maximum" : "maximum " + c.maximum);
          if (!schemaOf(h, "taskq.admission/1")) return "Legacy guard \xB7 Fixed limits";
          if (c.mode === "auto") return "Auto \xB7 " + (c.maximum == null ? "no host maximum" : "host maximum " + c.maximum);
          if (c.mode === "fixed") return "Fixed";
          return "Policy unknown";
        },
        gate: function(group, now) {
          const p = scopes(group).observation, f = freshness(group, now);
          if (!p) return "Unknown: no capacity status";
          if (!f.fresh) return "Unknown: status " + (f.status == null ? "missing" : age(f.status) + " old");
          const g = p.effective_policy && p.effective_policy.load_gate, bs = p.blockers || [];
          if (bs.some((b) => b.code === "config")) return "Unknown: invalid config";
          if (g && g.enabled === false) return "Off";
          if (!g) return "Unknown: gate not reported";
          if (f.sample == null || f.sample > FRESH) return "Unknown: " + (f.sample == null ? "no sample yet" : "sample " + age(f.sample) + " old");
          if (bs.some((b) => b.code === "sample" || b.knowledge === "unknown")) return "Unknown: " + bs.filter((b) => b.code === "sample" || b.knowledge === "unknown").map((b) => BLOCKERS[b.code] || BLOCKERS.unknown).join(", ");
          const held = bs.filter((b) => GATE.includes(b.code));
          return held.length ? "Waiting: " + [...new Set(held.map((b) => BLOCKERS[b.code]))].join(", ") : "Ready";
        },
        age: function(s) {
          return s < 120 ? s + " s" : s < 7200 ? Math.round(s / 60) + " min" : s < 172800 ? Math.round(s / 3600) + " h" : Math.round(s / 86400) + " d";
        },
        usage: function(group, now) {
          const p = scopes(group).observation, fresh2 = freshness(group, now).fresh;
          return (p && p.runtime_usage || []).map((u) => {
            const used = fresh2 && u.used != null ? u.used : null, l = u.limit;
            const limit = u.limit_kind === "unbounded" ? "no cap" : u.limit_kind === "unknown" || l == null && u.limit_kind !== "disabled" ? "?" : l == null ? "none" : String(l);
            const over = used != null && l != null && u.limit_kind !== "unbounded" && used > l;
            return {
              runtime: u.runtime_id,
              used,
              limit,
              over,
              off: u.limit_kind === "disabled",
              text: (used == null ? "?" : used) + "/" + limit + (over ? " \xB7 over limit" : u.limit_kind === "disabled" ? " \xB7 off" : "")
            };
          });
        },
        applied: function(group, now) {
          const t = group.targets || [];
          if (!t.length || !freshness(group, now).fresh || t.some((x) => x.effective_revision == null)) return "unknown";
          return t.every((x) => x.current_revision === x.effective_revision) ? "applied" : "saved";
        },
        headline: function(group, now) {
          return policy(group) + " \xB7 Load gate: " + gate(group, now);
        },
        fields: function(group) {
          const s = scopes(group), out = [], hc = s.host && s.host.current_policy || {};
          if (editable(s.host, "mode")) out.push({ key: "host.mode", target: s.host, field: "mode", label: "Mode", value: hc.mode, choice: ["auto", "fixed"] });
          if (editable(s.host, "maximum")) out.push({ key: "host.maximum", target: s.host, field: "maximum", label: "Host maximum", nullable: true, value: hc.maximum, none: "no maximum" });
          for (const r of s.runtimes) if (editable(r, "runtime_limit")) out.push({
            key: "rt:" + r.target.runtime_id,
            target: r,
            field: "runtime_limit",
            label: rtName(r.target.runtime_id),
            nullable: ["taskq.admission/1", "taskq.host-capacity/1", "taskq.project-limits/1"].includes(schemaFor(r, "runtime_limit")),
            none: { "taskq.host-capacity/1": "no cap", "taskq.project-limits/1": "no override" }[schemaFor(r, "runtime_limit")] || null,
            value: r.current_policy && r.current_policy.runtime_limit
          });
          const target = editable(s.host, "load_gate") ? s.host : s.project;
          if (editable(target, "load_gate")) {
            const g = target.current_policy && target.current_policy.load_gate || {};
            out.push({
              key: "gate.enabled",
              target,
              field: "load_gate",
              label: "Load gate",
              value: g.enabled,
              bool: true,
              locked: scopes(group).host && scopes(group).host.current_policy && scopes(group).host.current_policy.mode === "auto"
            });
            out.push({ key: "gate.cpu_max", target, field: "load_gate", label: "CPU ceiling %", value: g.cpu_max, lo: 10, hi: 100 });
            out.push({ key: "gate.memory_min", target, field: "load_gate", label: "Memory floor %", value: g.memory_min, lo: 1, hi: 95 });
          }
          return out;
        },
        edited: function(group, d) {
          return fields(group).filter((f) => d.touched[f.key] && d.values[f.key] !== shown(f));
        },
        problem: function(group, d) {
          const n = (v, lo, hi) => /^\d+$/.test(v) && Number(v) >= lo && Number(v) <= hi;
          for (const f of edited(group, d)) {
            const v = d.values[f.key];
            if (f.bool) {
              if (f.locked && !v && d.values["host.mode"] !== "fixed") return "Auto keeps the load gate on";
              continue;
            }
            if (f.choice) {
              if (!f.choice.includes(v)) return f.label + ": Auto or Fixed";
              continue;
            }
            if (f.field === "maximum" && v !== "" && !n(v, 0, 4294967295)) return f.label + ": an integer 0 or more, or empty for no maximum";
            if (f.field === "runtime_limit" && v === "") {
              if (!f.nullable) return f.label + ": Fixed limits need a number; 0 admits none";
              continue;
            }
            if (f.field === "runtime_limit" && !n(v, 0, 4294967295)) return f.label + ": an integer 0 or more";
            if (f.lo != null && !n(v, f.lo, f.hi)) return f.label + ": an integer " + f.lo + ".." + f.hi;
          }
          if (!edited(group, d).length) return "No change";
          const now = (k) => {
            const f = fields(group).find((x) => x.key === k);
            return f && (d.touched[k] ? d.values[k] : shown(f));
          };
          return now("host.mode") === "auto" && now("gate.enabled") === false ? "Auto keeps the load gate on" : null;
        },
        changes: function(group, d) {
          const by = /* @__PURE__ */ new Map();
          for (const f of edited(group, d)) {
            const k = tkey(f.target);
            if (!by.has(k)) by.set(k, {
              target: f.target.target,
              policy_schema: schemaFor(f.target, f.field),
              expected_revision: f.target.current_revision,
              changes: {}
            });
            const c = by.get(k).changes;
            if (f.field === "runtime_limit") c.runtime_limit = d.values[f.key] === "" ? null : Number(d.values[f.key]);
            else if (f.field === "mode") c.mode = d.values[f.key];
            else if (f.field === "maximum") c.maximum = d.values[f.key] === "" ? null : Number(d.values[f.key]);
            else {
              const g = fields(group).filter((x) => x.field === "load_gate");
              const v = (x) => d.touched[x.key] ? d.values[x.key] : shown(x);
              c.load_gate = { enabled: !!v(g[0]), cpu_max: Number(v(g[1])), memory_min: Number(v(g[2])) };
            }
          }
          return [...by.values()];
        },
        review: function(group, d) {
          return edited(group, d).map((f) => f.label + " " + said(f) + " \u2192 " + word(f, d.values[f.key]));
        },
        fresh: function() {
          return { draft: null, op: null, receipt: null };
        },
        status: function(group, local, cap, now) {
          if (local.receipt?.state === "saved" && (schemaOf(scopes(group).host, "taskq.host-capacity/1") || (local.op?.ops || []).some((o) => o.policy_schema === "taskq.project-limits/1"))) {
            const revision = local.receipt.saved_revision;
            if (typeof revision !== "string") return "unknown";
            const targets = (local.op?.ops || []).map((op) => ({ op, target: (group.targets || []).find((t) => tkey(t) === tkey({ target: op.target })) }));
            if (!targets.length || targets.some((x) => !x.target) || !freshness(group, now).fresh) return "saved";
            if (targets.every((x) => x.target.current_revision === revision && x.target.effective_revision === revision)) return "applied";
            return targets.some((x) => ![revision, x.op.expected_revision].includes(x.target.current_revision)) ? "superseded" : "saved";
          }
          if (local.receipt && local.receipt.state === "saved") return applied(group, now) === "applied" && JSON.stringify(revs(group)) !== JSON.stringify(local.receipt.from) ? "applied" : "saved";
          if (local.receipt) return local.receipt.state;
          if (!local.draft) return cap && cap.edit && fields(group).length ? "view" : "read-only";
          if (JSON.stringify(local.draft.base) !== JSON.stringify(revs(group))) return "conflict";
          return local.draft.review ? "review" : "editing";
        },
        step: function(group, local, cap, ev, newId, now) {
          const st = status(group, local, cap, now), l = { ...local };
          switch (ev.type) {
            case "open":
              if (!["view", "applied", "rejected", "superseded"].includes(st)) return { local, send: null };
              return { local: { draft: { base: revs(group), values: Object.fromEntries(fields(group).map((f) => [f.key, shown(f)])), touched: {}, review: false, error: null }, op: null, receipt: null }, send: null };
            case "input":
              if (!l.draft || ["sending"].includes(st)) return { local, send: null };
              l.draft = { ...l.draft, values: { ...l.draft.values, [ev.key]: ev.value }, touched: { ...l.draft.touched, [ev.key]: true }, review: false, error: null };
              return { local: l, send: null };
            case "review":
              if (st !== "editing") return { local, send: null };
              l.draft = { ...l.draft, error: problem(group, l.draft) };
              l.draft.review = !l.draft.error;
              return { local: l, send: null };
            case "back":
              if (st === "review") l.draft = { ...l.draft, review: false };
              return { local: l, send: null };
            case "rebase":
              if (st === "conflict") l.draft = { ...l.draft, base: revs(group), review: false };
              return { local: l, send: null };
            case "cancel":
              return ["editing", "review", "conflict"].includes(st) ? { local: fresh(), send: null } : { local, send: null };
            case "confirm": {
              if (st !== "review") return { local, send: null };
              l.op = { id: newId(), ops: changes(group, l.draft) };
              l.receipt = { state: "sending", from: revs(group) };
              l.draft = null;
              return { local: l, send: l.op };
            }
            case "receipt":
              if (!l.op || ev.opId !== l.op.id) return { local, send: null };
              l.receipt = { ...l.receipt, state: ["saved", "rejected", "unknown"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason, saved_revision: ev.saved_revision };
              return { local: l, send: null };
            case "retry":
              return st === "unknown" ? { local: { ...l, receipt: { ...l.receipt, state: "sending", reason: null } }, send: l.op } : { local, send: null };
            case "dismiss":
              return ["applied", "rejected", "superseded"].includes(st) ? { local: fresh(), send: null } : { local, send: null };
            default:
              return { local, send: null };
          }
        },
        render: function(group, local, cap, now) {
          const st = status(group, local, cap, now), f = freshness(group, now), s = scopes(group), id = "rsc-" + String(group.key || "").replace(/[^\w-]/g, "_");
          let h = '<section class="rsc" data-rsc-state="' + st + '" aria-label="' + esc("Capacity of " + (schemaOf(s.host, "taskq.host-capacity/1") ? "all registered projects" : group.labels?.project || "project") + " on " + (group.labels?.host || "this host")) + '">';
          h += '<p class="rsc-row"><span>Policy</span><span class="rsc-muted">' + esc(policy(group)) + "</span></p>";
          for (const u of usage(group, now)) h += '<p class="rsc-row' + (u.over ? " rsc-over" : "") + '" aria-label="' + esc(rtName(u.runtime) + ": " + (u.used == null ? "unknown" : u.used) + " used of " + u.limit + (u.over ? ", over limit; running tasks continue" : "")) + '"><span>' + esc(rtName(u.runtime)) + '</span><span class="rsc-count">' + esc(u.text) + "</span></p>";
          h += '<p class="rsc-row"><span>Load gate</span><span class="rsc-muted">' + esc(gate(group, now)) + "</span></p>";
          const sm = s.observation && s.observation.sample;
          if (sm && (sm.cpu_busy_percent != null || sm.memory_available_percent != null)) {
            const parts = [sm.cpu_busy_percent != null ? "CPU " + Math.round(sm.cpu_busy_percent) + " %" : "CPU unknown", sm.memory_available_percent != null ? "available memory " + Math.round(sm.memory_available_percent) + " %" : "memory unknown"];
            h += '<p class="rsc-muted">' + esc(parts.join(" \xB7 ") + (f.sample != null ? " \xB7 " + (f.sample > FRESH ? "last sample " + age(f.sample) + " ago" : age(f.sample) + " ago") : "")) + "</p>";
          }
          const other = (s.observation && s.observation.blockers || []).filter((b) => !GATE.includes(b.code) && b.code !== "sample" && b.code !== "config");
          if (other.length) h += '<p class="rsc-muted">Also blocked: ' + esc(other.map((b) => BLOCKERS[b.code] || BLOCKERS.unknown).join(", ")) + "</p>";
          if (s.observation && s.observation.completeness !== "complete") h += '<p class="rsc-muted">Blockers ' + esc(s.observation.completeness === "partial" ? "partly unknown" : "unknown") + "</p>";
          if (schemaOf(s.host, "taskq.host-capacity/1")) {
            const limits = s.project?.current_policy?.limits;
            h += '<p class="rsc-muted">Project ceilings (' + esc(group.labels?.project || "project") + "): " + esc(limits == null ? "unknown" : Object.entries(limits).map(([rt, n]) => rtName(rt) + " " + n).join(", ") || "none") + "</p>";
          }
          if (cap && cap.note) h += '<p class="rsc-alert">' + esc(cap.note) + "</p>";
          if (st === "view" || st === "read-only") {
            const a = applied(group, now);
            if (a === "saved") h += '<p class="rsc-muted" role="status">' + RECEIPT.saved + "</p>";
            return h + (st === "view" ? '<button type="button" data-rsc="open">Edit limits</button>' : cap && cap.reason ? '<p class="rsc-muted">' + esc(cap.reason) + "</p>" : "") + "</section>";
          }
          if (["sending", "saved", "applied", "rejected", "unknown", "superseded"].includes(st)) {
            const r = local.receipt;
            h += '<p class="rsc-receipt rsc-' + st + '" role="status" aria-live="polite">' + esc(RECEIPT[st] + (r && r.reason ? ": " + r.reason : "")) + "</p>";
            if (st === "unknown") h += '<button type="button" data-rsc="retry">Check again</button>';
            if (st === "applied" || st === "rejected" || st === "superseded") h += '<button type="button" data-rsc="open">Edit limits</button>';
            return h + "</section>";
          }
          const d = local.draft;
          if (st === "review") return h + '<div class="rsc-review" role="alert"><p>' + (schemaOf(s.host, "taskq.host-capacity/1") ? "Apply host limits to all registered projects on " + esc(group.labels?.host || "this host") : "Apply to " + esc(group.labels?.project || "project") + " on " + esc(group.labels?.host || "this host")) + "?</p>" + review(group, d).map((x) => "<p>" + esc(x) + "</p>").join("") + '<p class="rsc-muted">Running tasks continue. Other hosts and unchanged settings stay.</p></div><div class="rsc-actions"><button type="button" data-rsc="confirm">Confirm</button><button type="button" data-rsc="back">Back</button><button type="button" data-rsc="cancel">Cancel</button></div></section>';
          h += '<div class="rsc-form">' + fields(group).map((x) => input(id, x, d.values[x.key])).join("") + "</div>";
          if (st === "conflict") h += '<p class="rsc-alert" role="alert">' + RECEIPT.conflict + "</p>";
          if (d.error) h += '<p class="rsc-alert" role="alert">' + esc(d.error) + "</p>";
          return h + '<div class="rsc-actions">' + (st === "conflict" ? '<button type="button" data-rsc="rebase">Review new values</button>' : '<button type="button" data-rsc="review">Apply\u2026</button>') + '<button type="button" data-rsc="cancel">Cancel</button></div></section>';
        },
        input: function(id, x, v, mixed, off) {
          const fid = id + "-" + x.key.replace(/[^\w-]/g, "_"), dis = off ? " disabled" : "";
          let h = '<label for="' + fid + '">' + esc(x.label) + "</label>";
          if (x.choice) return h + '<select id="' + fid + '" data-rsc-field="' + esc(x.key) + '"' + dis + ">" + (mixed ? '<option value=""' + (v === "" ? " selected" : "") + ">Mixed</option>" : "") + x.choice.map((c) => '<option value="' + c + '"' + (v === c ? " selected" : "") + ">" + MODES[c] + "</option>").join("") + "</select>";
          if (x.bool) return h + '<input id="' + fid + '" type="checkbox" role="switch" data-rsc-field="' + esc(x.key) + '"' + (v ? " checked" : "") + (x.locked || off ? " disabled" : "") + (x.locked ? ' aria-describedby="' + fid + '-why"' : "") + (mixed ? ' aria-label="' + esc(x.label) + ': Mixed"' : "") + ">" + (x.locked ? '<span class="rsc-why" id="' + fid + '-why">Auto keeps the load gate on</span>' : mixed ? '<span class="rsc-why">Mixed</span>' : "");
          return h + '<input id="' + fid + '" type="number" inputmode="numeric" step="1" min="0" data-rsc-field="' + esc(x.key) + '" value="' + esc(v) + '"' + dis + (mixed ? ' placeholder="Mixed"' : x.value == null ? ' placeholder="' + esc(x.none || "none") + '"' : "") + ">";
        },
        mount: function(el, adapter) {
          const apply = function(ev) {
            const r = step(group, local, adapter.capability(group), ev, newId, now());
            local = r.local;
            if (ev.type !== "input") paint();
            if (r.send) ui.effect("resource.send", () => adapter.send(group, r.send), (x) => apply({ type: "receipt", opId: r.send.id, ...x }));
          }, paint = function() {
            const a = document.activeElement, focus = a && el.contains(a) ? { sel: a.dataset.rscField ? '[data-rsc-field="' + a.dataset.rscField + '"]' : '[data-rsc="' + a.dataset.rsc + '"]', s: a.type === "number" ? null : a.selectionStart, e: a.selectionEnd } : null;
            el.innerHTML = render(group, local, adapter.capability(group), now());
            const t = focus && el.querySelector(focus.sel);
            if (t) {
              t.focus();
              if (focus.s != null && t.setSelectionRange) try {
                t.setSelectionRange(focus.s, focus.e);
              } catch (_) {
              }
            }
          };
          let group = null, local = fresh();
          const now = () => adapter.now ? adapter.now() : Math.floor(Date.now() / 1e3);
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "rsc-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e) => {
            const b = e.target.closest("button[data-rsc]");
            if (b && !b.disabled) apply({ type: b.dataset.rsc });
          });
          ui.listen(el, "input", (e) => {
            const k = e.target.dataset.rscField;
            if (k) apply({ type: "input", key: k, value: e.target.type === "checkbox" ? e.target.checked : e.target.value });
          });
          return execution.module("resource-controls.mount.controller", {
            update(g) {
              group = g;
              paint();
            },
            editing() {
              return !!local.draft;
            }
          });
        },
        workFields: function(groups) {
          const by = /* @__PURE__ */ new Map();
          for (const g of groups) for (const f of fields(g)) {
            const x = by.get(f.key) || { ...f, values: [], locked: false };
            x.values.push(shown(f));
            x.locked = x.locked || !!f.locked;
            by.set(f.key, x);
          }
          return [...by.values()].map((f) => ({ ...f, mixed: new Set(f.values.map(String)).size > 1 || f.values.length < groups.length })).sort((a, b) => RANK(a.key) - RANK(b.key));
        },
        freshWork: function(selected) {
          return { selected: { ...selected }, draft: null, locals: {} };
        },
        workStatus: function(groups, w, cap, now) {
          const sel = picked(groups, w), sent = sel.filter((g) => w.locals[g.key]);
          if (sent.length) return { state: "receipts", each: Object.fromEntries(sent.map((g) => [g.key, status(g, w.locals[g.key], cap(g), now)])) };
          if (sel.some((g) => schemaOf(scopes(g).host, "taskq.host-capacity/1"))) return { state: "read-only" };
          if (!w.draft) return { state: sel.length && sel.every((g) => cap(g) && cap(g).edit) ? "view" : "read-only" };
          if (JSON.stringify(w.draft.base) !== JSON.stringify(workRevs(sel))) return { state: "conflict" };
          return { state: w.draft.review ? "review" : "editing" };
        },
        workStep: function(groups, w, cap, ev, newId, now) {
          const st = workStatus(groups, w, cap, now).state, l = { ...w }, none = { local: w, send: [] };
          const values = (gs) => Object.fromEntries(workFields(gs).map((f) => [f.key, f.mixed ? f.bool ? false : "" : f.values[0]]));
          switch (ev.type) {
            case "select": {
              if (!["view", "read-only", "editing"].includes(st)) return none;
              l.selected = { ...w.selected, [ev.key]: !!ev.on };
              if (l.draft) {
                const gs = picked(groups, l);
                l.draft = { ...l.draft, base: workRevs(gs), values: { ...values(gs), ...Object.fromEntries(Object.keys(l.draft.touched).map((k) => [k, l.draft.values[k]])) }, error: null };
              }
              return { local: l, send: [] };
            }
            case "open":
              if (st !== "view") return none;
              {
                const gs = picked(groups, w);
                l.draft = { base: workRevs(gs), values: values(gs), touched: {}, review: false, error: null };
              }
              return { local: l, send: [] };
            case "input":
              if (!l.draft) return none;
              l.draft = { ...l.draft, values: { ...l.draft.values, [ev.key]: ev.value }, touched: { ...l.draft.touched, [ev.key]: true }, review: false, error: null };
              return { local: l, send: [] };
            case "review": {
              if (st !== "editing") return none;
              const errs = picked(groups, w).map((g) => [g, problem(g, l.draft)]), bad = errs.find(([, e]) => e && e !== "No change");
              l.draft = { ...l.draft, error: bad ? (bad[0].labels && bad[0].labels.project || bad[0].key) + ": " + bad[1] : errs.some(([, e]) => !e) ? null : "No change" };
              l.draft.review = !l.draft.error;
              return { local: l, send: [] };
            }
            case "back":
              if (st === "review") l.draft = { ...l.draft, review: false };
              return { local: l, send: [] };
            case "rebase":
              if (st === "conflict") l.draft = { ...l.draft, base: workRevs(picked(groups, w)), review: false };
              return { local: l, send: [] };
            case "cancel":
              return ["editing", "review", "conflict"].includes(st) ? { local: freshWork(w.selected), send: [] } : none;
            case "confirm": {
              if (st !== "review") return none;
              const send = [];
              l.locals = {};
              for (const g of picked(groups, w)) {
                const ops = changes(g, w.draft);
                if (!ops.length) continue;
                const op = { id: newId(), ops };
                l.locals[g.key] = { draft: null, op, receipt: { state: "sending", from: revs(g) } };
                send.push({ key: g.key, op });
              }
              l.draft = null;
              return { local: l, send };
            }
            case "receipt": {
              const x = w.locals[ev.key];
              if (!x || !x.op || ev.opId !== x.op.id) return none;
              l.locals = { ...w.locals, [ev.key]: { ...x, receipt: { ...x.receipt, state: ["saved", "rejected", "unknown"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason } } };
              return { local: l, send: [] };
            }
            case "retry": {
              const x = w.locals[ev.key], g = groups.find((g2) => g2.key === ev.key);
              if (!x || !g || status(g, x, cap(g), now) !== "unknown") return none;
              l.locals = { ...w.locals, [ev.key]: { ...x, receipt: { ...x.receipt, state: "sending", reason: null } } };
              return { local: l, send: [{ key: ev.key, op: x.op }] };
            }
            case "dismiss": {
              const each = workStatus(groups, w, cap, now).each || {};
              return Object.values(each).every((s) => s === "applied" || s === "rejected") ? { local: freshWork(w.selected), send: [] } : none;
            }
            default:
              return none;
          }
        },
        workRender: function(groups, w, cap, now, host) {
          const ws = workStatus(groups, w, cap, now), st = ws.state, sel = picked(groups, w), d = w.draft;
          const busy = st === "review" || st === "receipts";
          let h = '<section class="rsc rsc-work" data-rsc-state="' + st + '" aria-label="' + esc("Work mode on " + (host || "this host")) + '">';
          h += '<p class="rsc-row"><span>Work mode</span><span class="rsc-muted">' + esc((host || "this host") + " \xB7 selected projects") + "</span></p>";
          h += '<fieldset class="rsc-scope"><legend>Projects</legend>' + groups.map((g) => '<label><input type="checkbox" data-rsc-select="' + esc(g.key) + '"' + (w.selected[g.key] ? " checked" : "") + (busy ? " disabled" : "") + ">" + esc(g.labels && g.labels.project || g.key) + "</label>").join("") + '<p class="rsc-muted">Other projects remain unchanged.</p></fieldset>';
          if (sel.length) h += '<p class="rsc-muted">' + esc(sel.map((g) => (g.labels && g.labels.project || g.key) + ": " + policy(g)).join(" \xB7 ")) + "</p>";
          if (st === "receipts") {
            for (const g of sel.filter((g2) => ws.each[g2.key])) {
              const s = ws.each[g.key], r = w.locals[g.key].receipt;
              h += '<p class="rsc-receipt rsc-' + s + '" role="status" aria-live="polite">' + esc((g.labels && g.labels.project || g.key) + ": " + RECEIPT[s] + (r && r.reason ? ": " + r.reason : "")) + "</p>" + (s === "unknown" ? '<button type="button" data-rsc="retry" data-rsc-key="' + esc(g.key) + '">Check again</button>' : "");
            }
            const done = Object.values(ws.each).every((s) => s === "applied" || s === "rejected");
            return h + (done ? '<button type="button" data-rsc="dismiss">Done</button>' : "") + "</section>";
          }
          if (st === "view" || st === "read-only") return h + '<button type="button" data-rsc="open"' + (st === "view" ? "" : " disabled") + ">Edit work mode</button>" + (st === "read-only" ? '<p class="rsc-muted">' + (sel.length ? "A selected project is read-only here" : "Select projects to change") + "</p>" : "") + "</section>";
          if (st === "review") return h + '<div class="rsc-review" role="alert"><p>' + esc("Apply on " + (host || "this host") + "?") + "</p>" + sel.map((g) => {
            const r = review(g, d);
            return "<p>" + esc((g.labels && g.labels.project || g.key) + ": " + (r.length ? r.join(", ") : "unchanged")) + "</p>";
          }).join("") + '<p class="rsc-muted">Running tasks continue. Other projects, hosts and unchanged settings stay.</p></div><div class="rsc-actions"><button type="button" data-rsc="confirm">Confirm</button><button type="button" data-rsc="back">Back</button><button type="button" data-rsc="cancel">Cancel</button></div></section>';
          h += '<div class="rsc-form">' + workFields(sel).map((f) => input("rsc-work", f, d.values[f.key], f.mixed && !d.touched[f.key], !sel.length)).join("") + "</div>";
          if (st === "conflict") h += '<p class="rsc-alert" role="alert">' + RECEIPT.conflict + "</p>";
          if (d.error) h += '<p class="rsc-alert" role="alert">' + esc(d.error) + "</p>";
          return h + '<div class="rsc-actions">' + (st === "conflict" ? '<button type="button" data-rsc="rebase">Review new values</button>' : '<button type="button" data-rsc="review">Review changes</button>') + '<button type="button" data-rsc="cancel">Cancel</button></div></section>';
        },
        mountWork: function(el, adapter, selected) {
          const apply = function(ev) {
            const r = workStep(groups, w, adapter.capability, ev, newId, now());
            w = r.local;
            if (ev.type !== "input") paint();
            for (const x of r.send) {
              const g = groups.find((g2) => g2.key === x.key);
              ui.effect("resource.work.send", () => adapter.send(g, x.op), (y) => apply({ type: "receipt", key: x.key, opId: x.op.id, ...y }));
            }
          }, paint = function() {
            const a = document.activeElement, sel = a && el.contains(a) ? a.dataset.rscField ? '[data-rsc-field="' + a.dataset.rscField + '"]' : a.dataset.rscSelect ? '[data-rsc-select="' + CSS.escape(a.dataset.rscSelect) + '"]' : '[data-rsc="' + a.dataset.rsc + '"]' : null;
            el.innerHTML = workRender(groups, w, adapter.capability, now(), adapter.host);
            const t = sel && el.querySelector(sel);
            if (t) t.focus();
          };
          let groups = [], w = freshWork(selected || {});
          const now = () => adapter.now ? adapter.now() : Math.floor(Date.now() / 1e3);
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "rsc-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e) => {
            const b = e.target.closest("button[data-rsc]");
            if (b && !b.disabled) apply({ type: b.dataset.rsc, key: b.dataset.rscKey });
          });
          ui.listen(el, "change", (e) => {
            const k = e.target.dataset.rscSelect;
            if (k) apply({ type: "select", key: k, on: e.target.checked });
          });
          ui.listen(el, "input", (e) => {
            const k = e.target.dataset.rscField;
            if (k) apply({ type: "input", key: k, value: e.target.type === "checkbox" ? e.target.checked : e.target.value });
          });
          return execution.module("resource-controls.mountWork.controller", { update(gs) {
            groups = gs;
            paint();
          }, editing() {
            return !!w.draft;
          } });
        },
        replayWork: function(fx, capability) {
          let groups = fx.groups, w = freshWork(fx.selected || {}), n = 0;
          const sent = [];
          const out = [];
          const cap = (g) => capability(g) || fx.capability;
          for (const ev of fx.events || []) {
            if (ev.type === "refresh") {
              groups = ev.groups.map((i) => fx.refresh[i]);
              continue;
            }
            if (ev.type === "check") {
              const s = workStatus(groups, w, cap, fx.now);
              out.push({ status: s.state, each: s.each || {}, html: workRender(groups, w, cap, fx.now, fx.host), sent: sent.slice() });
              continue;
            }
            const e = ev.type === "receipt" ? { ...ev, opId: (sent.filter((x) => x.key === ev.key).pop() || { op: {} }).op.id } : ev;
            const r = workStep(groups, w, cap, e, () => "op-" + ++n, fx.now);
            w = r.local;
            sent.push(...r.send);
          }
          return out;
        },
        replay: function(fx, capability) {
          let group = fx.groups[0], local = fresh(), n = 0;
          const sent = [];
          const out = [];
          const cap = (g) => capability(g) || fx.capability;
          for (const ev of fx.events || []) {
            if (ev.type === "refresh") {
              group = fx.groups[ev.group];
              continue;
            }
            if (ev.type === "check") {
              out.push({ status: status(group, local, cap(group), fx.now), html: render(group, local, cap(group), fx.now), sent: sent.slice(), headline: headline(group, fx.now) });
              continue;
            }
            const r = step(group, local, cap(group), ev.type === "receipt" ? { ...ev, opId: sent.length && sent[sent.length - 1].id } : ev, () => "op-" + ++n, fx.now);
            local = r.local;
            if (r.send) sent.push(r.send);
          }
          return out;
        }
      });
      var root = globalThis;
      var STATES = ["view", "editing", "review", "conflict", "sending", "saved", "applied", "rejected", "unknown", "read-only", "superseded"];
      var FRESH = 60;
      var { esc } = execution.module("resource-controls", { esc: (s) => String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]) });
      var { rtName } = execution.module("resource-controls", { rtName: (rt) => ({ claude: "Claude", codex: "Codex", "codex-app": "Codex app" })[rt] || rt });
      var BLOCKERS = {
        cpu: "CPU",
        memory: "memory",
        pressure: "memory pressure",
        sample: "no fresh sample",
        config: "invalid config",
        cooldown: "recovery",
        spacing: "host spacing",
        max: "host maximum",
        version: "old service",
        host: "host hold",
        runtime_disabled: "runtime disabled",
        pause: "paused",
        focus: "focus",
        dependency: "dependency",
        routing: "routing",
        access: "access",
        provider: "provider limit",
        unknown: "unknown reason"
      };
      var GATE = ["cpu", "memory", "pressure", "cooldown"];
      var { tkey } = execution.module("resource-controls", { tkey: (t) => JSON.stringify([t.target.installation_id ?? null, t.target.generation ?? null, t.target.project_id ?? null, t.target.runtime_id ?? null]) });
      var { schemaOf } = execution.module("resource-controls", { schemaOf: (x, s) => x && (x.supported || []).find((p) => p.policy_schema === s) });
      var { schemaFor } = execution.module("resource-controls", { schemaFor: (x, field) => {
        const p = x && (x.supported || []).find((p2) => p2.fields.includes(field) && p2.scopes.includes(x.target.runtime_id != null ? "runtime" : x.target.project_id != null ? "project" : "host"));
        return p ? p.policy_schema : null;
      } });
      var { editable } = execution.module("resource-controls", { editable: (x, field) => !!schemaFor(x, field) });
      var MODES = { auto: "Auto", fixed: "Fixed" };
      var { shown } = execution.module("resource-controls", { shown: (f) => f.bool ? !!f.value : f.value == null ? "" : String(f.value) });
      var { word } = execution.module("resource-controls", { word: (f, v) => f.bool ? v ? "on" : "off" : f.choice ? MODES[v] || "unknown" : v === "" ? f.none || "none" : v });
      var { said } = execution.module("resource-controls", { said: (f) => word(f, shown(f)) });
      var { revs } = execution.module("resource-controls", { revs: (group) => Object.fromEntries((group.targets || []).map((t) => [tkey(t), t.current_revision])) });
      var RECEIPT = {
        sending: "Saving\u2026",
        saved: "Saved; waiting for service",
        applied: "Applied: read back from the service",
        superseded: "Saved; replaced by a newer command",
        rejected: "Not saved",
        unknown: "Not confirmed; the new limits may not be saved. Check again",
        conflict: "Settings changed since you opened the editor"
      };
      var { RANK } = execution.module("resource-controls", { RANK: (k) => k.startsWith("host.") ? 0 : k.startsWith("rt:") ? 1 : 2 });
      var { picked } = execution.module("resource-controls", { picked: (groups, w) => groups.filter((g) => w.selected[g.key]) });
      var { workRevs } = execution.module("resource-controls", { workRevs: (gs) => Object.fromEntries(gs.map((g) => [g.key, revs(g)])) });
      var api = {
        STATES,
        scopes,
        freshness,
        policy,
        gate,
        usage,
        applied,
        headline,
        fields,
        changes,
        review,
        status,
        step,
        render,
        mount,
        replay,
        workFields,
        workStatus,
        workStep,
        workRender,
        mountWork,
        replayWork
      };
      module.exports = api;
    }
  });
  return require_resource_controls();
})();
