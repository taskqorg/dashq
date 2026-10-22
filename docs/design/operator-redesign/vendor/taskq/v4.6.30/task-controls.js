"use strict";
var TaskqTaskControls = (() => {
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

  // src/task-controls.js
  var require_task_controls = __commonJS({
    "src/task-controls.js"(exports, module) {
      var { execution } = require_ui_runtime();
      var LABELS = { start: "Start", stop: "Stop", later: "Later", retry: "Retry" };
      var { op, render, esc } = execution.module("task-controls", {
        esc: function(s) {
          return String(s).replace(/[&<>"']/g, (c) => "&#" + c.charCodeAt(0) + ";");
        },
        op: function(row, action, prefix) {
          return { id: prefix + "." + action + "." + row.n + "." + row.revision, task: row.n, revision: row.revision, action };
        },
        render: function(row, { key = "", url = "", except = [] } = {}) {
          const buttons = (row.controls || []).filter((a) => LABELS[a] && !except.includes(a)).map((a) => '<button data-a="control" data-control="' + a + '" data-key="' + esc(key) + '" aria-label="' + LABELS[a] + " #" + row.n + '">' + LABELS[a] + "</button>");
          if (url) buttons.push('<a href="' + esc(url) + '" data-url="' + esc(url) + '" aria-label="Close #' + row.n + ' on the board">Close on board \u2197</a>');
          return buttons.length ? '<span class="row task-controls">' + buttons.join("") + "</span>" : "";
        }
      });
      module.exports = { VERSION: "taskq.task-controls/1", LABELS, op, render };
    }
  });
  return require_task_controls();
})();
