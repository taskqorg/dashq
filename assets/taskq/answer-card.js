"use strict";
var TaskqAnswerCard = (() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __esm = (fn, res, err) => function __init() {
    if (err) throw err[0];
    try {
      return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
    } catch (e2) {
      throw err = [e2], e2;
    }
  };
  var __commonJS = (cb, mod) => function __require() {
    try {
      return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
    } catch (e2) {
      throw mod = 0, e2;
    }
  };
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

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
            } catch (_2) {
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
              id: () => Array.from(root.crypto.getRandomValues(new Uint8Array(16)), (b2) => b2.toString(16).padStart(2, "0")).join("")
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

  // node_modules/preact/dist/preact.mjs
  var preact_exports = {};
  __export(preact_exports, {
    Component: () => $,
    Fragment: () => M,
    cloneElement: () => k,
    createContext: () => K,
    createElement: () => g,
    createPortal: () => R,
    createRef: () => m,
    h: () => g,
    hydrate: () => J,
    isValidElement: () => i,
    options: () => n,
    render: () => G,
    toChildArray: () => A
  });
  function _(n2) {
    n2 && n2.parentNode && n2.remove();
  }
  function g(n2, t2, i2) {
    var r2, u2, f2, o2 = {}, e2 = arguments.length;
    for (f2 in t2) "key" == f2 ? r2 = t2[f2] : "ref" == f2 && "function" != typeof n2 ? u2 = t2[f2] : o2[f2] = t2[f2];
    return e2 > 2 && (o2.children = e2 > 3 ? d.call(arguments, 2) : i2), b(n2, o2, r2, u2, null);
  }
  function k(n2, t2, i2) {
    var r2, u2, f2, o2 = w({}, n2.props), e2 = arguments.length;
    for (f2 in t2) "key" == f2 ? r2 = t2[f2] : "ref" == f2 && "function" != typeof n2.type ? u2 = t2[f2] : o2[f2] = t2[f2];
    return e2 > 2 && (o2.children = e2 > 3 ? d.call(arguments, 2) : i2), b(n2.type, o2, void 0 !== r2 ? r2 : n2.key, void 0 !== u2 ? u2 : n2.ref, null);
  }
  function b(i2, r2, u2, f2, o2) {
    var e2 = { type: i2, props: r2, key: u2, ref: f2, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: o2 || ++t, __i: -1, __u: 0 };
    return !o2 && n.vnode && n.vnode(e2), e2;
  }
  function m() {
    return { current: null };
  }
  function M(n2) {
    return n2.children;
  }
  function $(n2, t2) {
    this.props = n2, this.context = t2, this.__g = 0;
  }
  function x(n2, t2) {
    if (null == t2) return n2.__ ? x(n2.__, n2.__i + 1) : null;
    for (var i2; t2 < n2.__k.length; t2++) if ((i2 = n2.__k[t2]) && i2.__e) return i2.__e;
    return "function" != typeof n2.type || n2.props.__P ? null : x(n2);
  }
  function S(n2) {
    if ((n2 = n2.__) && n2.__c && !n2.props.__P) return n2.__e = null, n2.__k.some(function(t2) {
      return t2 && (n2.__e = t2.__e);
    }), S(n2);
  }
  function C(t2) {
    (8 & t2.__g || !(t2.__g |= 8) || !r.push(t2) || f++) && u == n.debounceRendering || ((u = n.debounceRendering) || queueMicrotask)(j);
  }
  function j() {
    var t2, i2, u2, e2, l2, c2, a2, s2, h2;
    try {
      for (i2 = 1; r.length; ) r.length > i2 && r.sort(o), t2 = r.shift(), i2 = r.length, 8 & t2.__g && (e2 = void 0, l2 = void 0, c2 = (l2 = (u2 = t2).__v).__e, a2 = [], s2 = [], (h2 = u2.__P) && ((e2 = w({ constructor: void 0 }, l2)).__v = l2.__v + 1, n.vnode && n.vnode(e2), N(h2, e2, l2, u2.__n, h2.namespaceURI, 32 & l2.__u ? [c2] : null, a2, c2 || x(l2), 32 & l2.__u, s2), e2.__v = l2.__v, e2.__.__k[e2.__i] = e2, z(a2, e2, s2), l2.__ = l2.__e = null, e2.__e != c2 && S(e2)));
    } finally {
      r.length = f = 0;
    }
  }
  function L(n2, t2, i2, r2, u2, f2, o2, e2, l2, c2, a2) {
    var s2, v2, y2, d2, w2, _2, g2 = r2.__k || p, k2 = t2.length;
    for (l2 = H(i2, t2, g2, l2, k2), s2 = 0; s2 < k2; s2++) null != (y2 = i2.__k[s2]) && (v2 = ~y2.__i && g2[y2.__i] || h, y2.__i = s2, _2 = N(n2, y2, v2, u2, f2, o2, e2, l2, c2, a2), d2 = y2.__e, (v2.ref != y2.ref || 8 & v2.__u) && (v2.ref && D(v2.ref, null, y2, v2), y2.ref && a2.push(y2.ref, y2.__c || d2, y2)), w2 = w2 || d2, 4 & y2.__u ? (l2 = I(y2, l2, n2, !v2.__v), v2.__e && (v2.__e = null)) : "function" == typeof y2.type && void 0 !== _2 ? l2 = _2 : d2 && (l2 = d2.nextSibling), y2.__u &= -7);
    return i2.__e = w2, l2;
  }
  function H(n2, t2, i2, r2, u2) {
    var f2, o2, e2, l2, c2, a2, s2, h2, p2, v2, d2 = i2.length, w2 = d2, _2 = 0, g2 = false, k2 = n2.__k = Array(u2);
    for (f2 = 0; f2 < u2; f2++) null != (o2 = t2[f2]) && "boolean" != typeof o2 && "function" != typeof o2 ? ("object" != typeof o2 || o2.constructor == String ? o2 = k2[f2] = b(null, o2) : y(o2) ? o2 = k2[f2] = b(M, { children: o2 }) : void 0 === o2.constructor && o2.__b ? o2 = k2[f2] = b(o2.type, o2.props, o2.key, o2.ref, o2.__v) : k2[f2] = o2, l2 = f2 + _2, o2.__ = n2, o2.__b = n2.__b + 1, e2 = null, ~(c2 = o2.__i = O(o2, i2, l2, w2)) && (w2--, (e2 = i2[c2]) && (e2.__u |= 2)), e2 && e2.__v ? (o2.__u |= 2, c2 == l2 - 1 ? _2-- : c2 == l2 + 1 ? _2++ : c2 != l2 && (c2 > l2 ? _2-- : _2++, g2 = true)) : (~c2 || (u2 > d2 ? _2-- : u2 < d2 && _2++), "function" != typeof o2.type && (o2.__u |= 4))) : k2[f2] = null;
    if (g2) {
      for (a2 = [], s2 = [], f2 = 0; f2 < u2; f2++) if ((o2 = k2[f2]) && 2 & o2.__u) {
        for (h2 = 0, p2 = a2.length; h2 < p2; ) a2[v2 = h2 + p2 >> 1] < o2.__i ? h2 = v2 + 1 : p2 = v2;
        a2[h2] = o2.__i, s2[f2] = h2 + 1;
      }
      for (_2 = a2.length; f2--; ) s2[f2] && (s2[f2] == _2 ? _2-- : k2[f2].__u |= 4);
    }
    if (w2) for (f2 = 0; f2 < d2; f2++) !(e2 = i2[f2]) || 2 & e2.__u || (e2.__e == r2 && (r2 = x(e2)), E(e2, e2));
    return r2;
  }
  function I(n2, t2, i2, r2) {
    var u2, f2, o2;
    if ("function" == typeof n2.type) {
      if (n2.props.__P) return t2;
      if (u2 = n2.__k) for (f2 = 0; f2 < u2.length; f2++) u2[f2] && (u2[f2].__ = n2, t2 = I(u2[f2], t2, i2, false));
      return t2;
    }
    for (t2 && !t2.parentNode && (t2 = x(n2)) && !t2.parentNode && (t2 = null), o2 = t2; o2 && 8 == o2.nodeType; ) o2 = o2.nextSibling;
    for (n2.__e != o2 && (!r2 && i2.moveBefore && n2.__e.parentNode ? i2.moveBefore(n2.__e, t2) : i2.insertBefore(n2.__e, t2 || null)), t2 = n2.__e; (t2 = t2 && t2.nextSibling) && 8 == t2.nodeType; ) ;
    return t2;
  }
  function A(n2, t2) {
    return t2 = t2 || [], null != n2 && "boolean" != typeof n2 && (y(n2) ? n2.some(function(n3) {
      A(n3, t2);
    }) : t2.push(n2)), t2;
  }
  function O(n2, t2, i2, r2) {
    var u2, f2, o2, e2 = n2.key, l2 = n2.type, c2 = t2[i2], a2 = c2 && !(2 & c2.__u);
    if (null === c2 && null == e2 || a2 && e2 == c2.key && l2 == c2.type) return i2;
    if (r2 > (a2 ? 1 : 0)) {
      for (u2 = i2 - 1, f2 = i2 + 1; u2 >= 0 || f2 < t2.length; ) if ((c2 = t2[o2 = u2 >= 0 ? u2-- : f2++]) && !(2 & c2.__u) && e2 == c2.key && l2 == c2.type) return o2;
    }
    return -1;
  }
  function P(n2, t2, i2) {
    null == i2 && (i2 = ""), "-" == t2[0] ? n2.setProperty(t2, i2) : n2[t2] = i2;
  }
  function T(n2, t2, i2, r2, u2) {
    var f2, o2;
    n: if ("style" == t2) if ("string" == typeof i2) n2.style.cssText = i2;
    else {
      if ("string" == typeof r2 && (n2.style.cssText = r2 = ""), r2) for (t2 in r2) i2 && t2 in i2 || P(n2.style, t2, "");
      if (i2) for (t2 in i2) r2 && i2[t2] == r2[t2] || P(n2.style, t2, i2[t2]);
    }
    else if ("o" == t2[0] && "n" == t2[1]) (n2.__e || (n2.__e = {}))[t2] = i2, i2 && r2 || (o2 = a[t2] || (a[t2] = q(t2)), (n2.__a || (n2.__a = {}))[t2] = c, f2 = t2 != (t2 = t2.replace(l, "$1")), (t2 = t2.slice(2))[0] < "a" && (t2 = t2.toLowerCase()), i2 ? n2.addEventListener(t2, o2, f2) : n2.removeEventListener(t2, o2, f2));
    else {
      if ("http://www.w3.org/2000/svg" == u2) t2 = t2.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
      else if ("width" != t2 && "height" != t2 && "href" != t2 && "list" != t2 && "form" != t2 && "tabIndex" != t2 && "download" != t2 && "rowSpan" != t2 && "colSpan" != t2 && "role" != t2 && "popover" != t2 && t2 in n2) try {
        n2[t2] = null == i2 ? "" : i2;
        break n;
      } catch (n3) {
      }
      "function" == typeof i2 || (null == i2 || false === i2 && "-" != t2[4] ? n2.removeAttribute(t2) : n2.setAttribute(t2, "popover" == t2 && 1 == i2 ? "" : i2));
    }
  }
  function q(t2) {
    return function(i2) {
      if (this.__e) {
        var r2 = this.__e[t2];
        if (null == i2[e]) i2[e] = c++;
        else if (i2[e] < this.__a[t2]) return;
        return r2(n.event ? n.event(i2) : i2);
      }
    };
  }
  function N(t2, i2, r2, u2, f2, o2, e2, l2, c2, a2) {
    var s2, h2, v2, d2, g2, k2, b2, m2, S2, C2, j2, H2, I2, A2, O2, P2, T2, q2, N2, z2, D2 = i2.type;
    if (void 0 !== i2.constructor) return null;
    if (128 & r2.__u && (c2 = 32 & r2.__u, s2 = r2.__c.__z)) {
      if (i2.__u |= c2, h2 = o2 = [], 8 == s2.nodeType) for (v2 = 1, d2 = s2.nextSibling; d2; d2 = d2.nextSibling) {
        if (8 == d2.nodeType) {
          if (d2.data.startsWith("$s")) v2++;
          else if (d2.data.startsWith("/$s") && !--v2) break;
        }
        o2.push(d2);
      }
      else o2.push(s2);
      l2 = o2[0];
    }
    (s2 = n.__b) && s2(i2);
    n: if ("function" == typeof D2) {
      g2 = e2.length;
      try {
        if (C2 = i2.props, j2 = (s2 = D2.prototype) && s2.render, H2 = (s2 = D2.contextType) && u2[s2.__c], I2 = s2 ? H2 ? H2.props.value : s2.__ : u2, r2.__c ? 2 & (k2 = i2.__c = r2.__c).__g && (k2.__g |= 1) : (j2 ? i2.__c = k2 = new D2(C2, I2) : (i2.__c = k2 = new $(C2, I2), k2.constructor = D2, k2.render = F), H2 && H2.sub(k2), k2.state || (k2.state = {}), k2.__n = u2, k2.__g |= 8, k2.__h = [], k2.__k = []), j2 && (k2.__s || (k2.__s = k2.state), D2.getDerivedStateFromProps && (k2.__s == k2.state && (k2.__s = w({}, k2.__s)), w(k2.__s, D2.getDerivedStateFromProps(C2, k2.__s)))), b2 = k2.props, m2 = k2.state, k2.__v = i2, r2.__c) {
          if (j2 && !D2.getDerivedStateFromProps && C2 !== b2 && k2.componentWillReceiveProps && k2.componentWillReceiveProps(C2, I2), i2.__v == r2.__v && !(8 & k2.__g) || !(4 & k2.__g) && k2.shouldComponentUpdate && false === k2.shouldComponentUpdate(C2, k2.__s, I2)) {
            i2.__v != r2.__v && (k2.props = C2, k2.state = k2.__s, k2.__g &= -9), i2.__e = r2.__e, i2.__k = r2.__k, i2.__k.some(function(n2) {
              n2 && (n2.__ = i2);
            }), p.push.apply(k2.__h, k2.__k), k2.__k = [], k2.__h.length && e2.push(k2), l2 = x(r2);
            break n;
          }
          k2.componentWillUpdate && k2.componentWillUpdate(C2, k2.__s, I2), j2 && k2.componentDidUpdate && k2.__h.push(function() {
            k2.componentDidUpdate(b2, m2, S2);
          });
        } else j2 && !D2.getDerivedStateFromProps && k2.componentWillMount && k2.componentWillMount(), j2 && k2.componentDidMount && k2.__h.push(k2.componentDidMount);
        if (k2.context = I2, k2.props = C2, k2.__P = t2, k2.__g &= -5, A2 = n.__r, O2 = 0, j2) k2.state = k2.__s, k2.__g &= -9, A2 && A2(i2), s2 = k2.render(k2.props, k2.state, k2.context), p.push.apply(k2.__h, k2.__k), k2.__k = [];
        else do {
          k2.__g &= -9, A2 && A2(i2), s2 = k2.render(k2.props, k2.state, k2.context), k2.state = k2.__s;
        } while (8 & k2.__g && ++O2 < 25);
        k2.state = k2.__s, k2.getChildContext && (u2 = w({}, u2, k2.getChildContext())), j2 && r2.__c && k2.getSnapshotBeforeUpdate && (S2 = k2.getSnapshotBeforeUpdate(b2, m2)), P2 = s2 && s2.type === M && null == s2.key ? s2.props.children : s2, C2.__P && (s2 = l2, f2 = (t2 = C2.__P).namespaceURI, c2 = o2 = null, r2.props && r2.props.__P != t2 && (r2.__k.some(function(n2) {
          n2 && E(n2, n2);
        }), r2.__k = null), l2 = r2.__k ? x(r2, 0) : null), l2 = L(t2, y(P2) ? P2 : [P2], i2, r2, u2, f2, o2, e2, l2, c2, a2), C2.__P && (i2.__e = null, l2 = s2), i2.__u &= -161, 128 & r2.__u && (k2.__z = null), h2 && h2.some(_), k2.__h.length && e2.push(k2), 1 & k2.__g && (k2.__g &= -4);
      } catch (t3) {
        if (e2.length = g2, i2.__v = null, c2 || o2) if (t3.then) {
          if (T2 = 0, i2.__u |= c2 ? 160 : 128, o2) {
            for (~(N2 = o2.indexOf(l2 || void 0)) || (N2 = o2.length); (z2 = o2[N2 - 1]) && 8 == z2.nodeType; ) N2--;
            for (; N2 < o2.length; N2++) if (z2 = o2[N2]) {
              if (o2[N2] = null, 8 == z2.nodeType) {
                if (z2.data.startsWith("$s")) T2++ || (q2 = z2);
                else if (T2 && z2.data.startsWith("/$s") && !--T2) {
                  l2 = z2;
                  break;
                }
              } else if (!T2) break;
            }
          }
          if (!q2) {
            for (; l2 && 8 == l2.nodeType && l2.nextSibling; ) l2 = l2.nextSibling;
            q2 = l2;
          }
          i2.__c.__z || (i2.__c.__z = q2), i2.__e = l2;
        } else o2 && o2.some(_);
        else i2.__e = r2.__e;
        i2.__k || (i2.__k = r2.__k || []), t3.then || V(i2), n.__e(t3, i2, r2);
      }
    } else l2 = i2.__e = B(r2.__e, i2, r2, u2, f2, o2, e2, c2, a2, t2);
    return (s2 = n.diffed) && s2(i2), 128 & i2.__u ? void 0 : l2;
  }
  function V(n2) {
    n2 && (n2.__c && (n2.__c.__g |= 4), n2.__k && n2.__k.some(V));
  }
  function z(t2, i2, r2) {
    for (var u2 = 0; u2 < r2.length; ) D(r2[u2++], r2[u2++], r2[u2++]);
    n.__c && n.__c(i2, t2), t2.some(function(i3) {
      try {
        t2 = i3.__h, i3.__h = [], t2.some(function(n2) {
          n2.call(i3);
        });
      } catch (t3) {
        n.__e(t3, i3.__v);
      }
    });
  }
  function B(t2, i2, r2, u2, f2, o2, e2, l2, c2, a2) {
    var s2, p2, w2, g2, k2, b2, m2, M2, $2, S2 = r2.props || h, C2 = i2.props, j2 = i2.type;
    if ("svg" == j2 ? f2 = "http://www.w3.org/2000/svg" : "math" == j2 ? f2 = "http://www.w3.org/1998/Math/MathML" : f2 || (f2 = "http://www.w3.org/1999/xhtml"), o2) {
      for (s2 = 0; s2 < o2.length; s2++) if ((k2 = o2[s2]) && (j2 ? k2.localName == j2 : 3 == k2.nodeType)) {
        t2 = k2, o2[s2] = null;
        break;
      }
    }
    if (!t2) {
      if (M2 = a2.ownerDocument || document, !j2) return M2.createTextNode(C2);
      t2 = M2.createElementNS(f2, j2, C2.is && C2), l2 && (n.__m && n.__m(i2, o2), l2 = false), o2 = null;
    }
    if (j2) {
      if (a2 = "template" == j2 ? t2.content : t2, o2 = "textarea" == j2 && null != C2.defaultValue ? null : o2 && d.call(a2.childNodes), !l2 && o2) for (S2 = {}, s2 = 0; s2 < t2.attributes.length; s2++) S2[(k2 = t2.attributes[s2]).name] = k2.value;
      for (s2 in S2) k2 = S2[s2], "dangerouslySetInnerHTML" == s2 ? w2 = k2 : "children" == s2 || s2 in C2 || "value" == s2 && "defaultValue" in C2 || "checked" == s2 && "defaultChecked" in C2 || T(t2, s2, null, k2, f2);
      for (s2 in $2 = 1 & r2.__u, C2) k2 = C2[s2], "children" == s2 ? g2 = k2 : "dangerouslySetInnerHTML" == s2 ? p2 = k2 : "value" == s2 ? b2 = k2 : "checked" == s2 ? m2 = k2 : l2 && "function" != typeof k2 || !(S2[s2] !== k2 || $2 && null != k2) || T(t2, s2, k2, S2[s2], f2);
      p2 ? (l2 || w2 && (p2.__html == w2.__html || p2.__html == t2.innerHTML) || (t2.innerHTML = p2.__html), i2.__k = []) : (w2 && (t2.textContent = ""), ("foreignObject" == j2 || "http://www.w3.org/1998/Math/MathML" == f2 && v.test(j2)) && (f2 = "http://www.w3.org/1999/xhtml"), L(a2, y(g2) ? g2 : [g2], i2, r2, u2, f2, o2, e2, o2 ? o2[0] : r2.__k && x(r2, 0), l2, c2), o2 && o2.some(_)), l2 && "textarea" != j2 || (s2 = "value", "progress" == j2 && null == b2 ? t2.removeAttribute(s2) : null == b2 || b2 === t2[s2] && ("progress" != j2 || b2) || T(t2, s2, b2, S2[s2], f2), s2 = "checked", null != m2 && m2 != t2[s2] && T(t2, s2, m2, S2[s2], f2));
    } else S2 === C2 || l2 && t2.data == C2 || (t2.data = C2);
    return t2;
  }
  function D(t2, i2, r2, u2) {
    try {
      "function" == typeof t2 ? i2 ? i2.__x = t2(i2) || 1 : u2 && (u2 = u2.__c || u2.__e) && (i2 = u2.__x) && (u2.__x = null, "function" == typeof i2 ? i2() : t2(null)) : t2.current = i2;
    } catch (t3) {
      n.__e(t3, r2);
    }
  }
  function E(t2, i2, r2) {
    var u2, f2;
    if (n.unmount && n.unmount(t2), !(u2 = t2.ref) || u2.current && u2.current != t2.__e || D(u2, null, i2, t2), u2 = t2.__c) {
      if (u2.componentWillUnmount) try {
        u2.componentWillUnmount();
      } catch (t3) {
        n.__e(t3, i2);
      }
      u2.__P = u2.__n = null;
    }
    if (u2 = t2.__k) for (f2 = 0; f2 < u2.length; f2++) u2[f2] && E(u2[f2], i2, "function" != typeof t2.type || r2 && !t2.props.__P);
    (u2 = t2.__e) && (r2 || _(u2), u2.__e && (u2.__e = null)), t2.__e = t2.__c = t2.__ = null;
  }
  function F(n2, t2, i2) {
    return this.constructor(n2, i2);
  }
  function G(t2, i2) {
    var r2, u2, f2, o2;
    n.__ && n.__(t2, i2), 9 == i2.nodeType && (i2 = i2.documentElement), u2 = (r2 = t2 && 32 & t2.__u) ? null : i2.__k, i2.__k = b(M, { children: [t2] }), f2 = [], o2 = [], N(i2, i2.__k, u2 || h, h, i2.namespaceURI, u2 ? null : i2.firstChild ? d.call(i2.childNodes) : null, f2, u2 ? u2.__e : i2.firstChild, r2, o2), z(f2, i2.__k, o2), i2.__k.props.children = null;
  }
  function J(n2, t2) {
    n2 && "object" == typeof n2 && (n2.__u |= 32), G(n2, t2);
  }
  function K(n2) {
    function t2(n3) {
      var i2, r2;
      return this.getChildContext || (i2 = /* @__PURE__ */ new Set(), (r2 = {})[t2.__c] = this, this.getChildContext = function() {
        return r2;
      }, this.shouldComponentUpdate = function(n4) {
        this.props.value != n4.value && i2.forEach(function(n5) {
          n5.__g |= 4, C(n5);
        });
      }, this.sub = function(n4) {
        i2.add(n4);
        var t3 = n4.componentWillUnmount;
        n4.componentWillUnmount = function() {
          i2.delete(n4), t3 && t3.call(n4);
        };
      }), n3.children;
    }
    return t2.__c = "__cC" + s++, t2.__ = n2, t2.Provider = (t2.Consumer = function(n3, t3) {
      return n3.children(t3);
    }).contextType = t2, t2;
  }
  function Q(n2) {
    return n2.children;
  }
  function R(n2, t2) {
    return b(Q, { __P: t2, children: n2 });
  }
  var n, t, i, r, u, f, o, e, l, c, a, s, h, p, v, y, d, w;
  var init_preact = __esm({
    "node_modules/preact/dist/preact.mjs"() {
      h = {};
      p = [];
      v = /^m(i|n|o|s|text|space)$/;
      y = Array.isArray;
      d = p.slice;
      w = Object.assign;
      n = { __e: function(n2, t2, i2, r2) {
        for (var u2, o2, e2; t2 = t2.__; ) if ((u2 = t2.__c) && !(1 & u2.__g)) {
          u2.__g |= 4;
          try {
            if ((o2 = u2.constructor) && o2.getDerivedStateFromError && (u2.setState(o2.getDerivedStateFromError(n2)), e2 = 8 & u2.__g), u2.componentDidCatch && (u2.componentDidCatch(n2, r2 || {}), e2 = 8 & u2.__g), e2) return void (u2.__g |= 2);
          } catch (t3) {
            n2 = t3, e2 = 0;
          }
        }
        throw f = 0, n2;
      } }, t = 0, i = function(n2) {
        return null != n2 && void 0 === n2.constructor;
      }, $.prototype.setState = function(n2, t2) {
        var i2 = this.__s;
        i2 && i2 != this.state || (i2 = this.__s = w({}, this.state)), "function" == typeof n2 && (n2 = n2(w({}, i2), this.props)), n2 && (w(i2, n2), this.__v && (t2 && this.__k.push(t2), C(this)));
      }, $.prototype.forceUpdate = function(n2) {
        this.__v && (this.__g |= 4, n2 && this.__h.push(n2), C(this));
      }, $.prototype.render = M, r = [], f = 0, o = function(n2, t2) {
        return n2.__v.__b - t2.__v.__b;
      }, e = /* @__PURE__ */ Symbol(), l = /(PointerCapture)$|Capture$/i, c = 0, a = {}, s = 0;
    }
  });

  // src/reply-files.js
  var require_reply_files = __commonJS({
    "src/reply-files.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var { execution } = ui;
      var { h: h2 } = (init_preact(), __toCommonJS(preact_exports));
      var waiting = /* @__PURE__ */ new Map();
      var api = execution.module("reply-files", {
        settle: function(id) {
          ui.cancel(waiting.get(id));
          waiting.delete(id);
        },
        problem: function(files) {
          const f2 = (files || []).find((f3) => f3.pending || f3.error);
          return f2 ? f2.error || f2.name + ": copying attachment; wait before Send" : "";
        },
        refuse: function(kind) {
          switch (kind) {
            case "read":
              execution.action("reply.files.refused.read", () => {
              })();
              break;
            case "storage":
              execution.action("reply.files.refused.storage", () => {
              })();
              break;
            default:
              execution.action("reply.files.refused.metadata", () => {
              })();
          }
        },
        read: function(files, put) {
          for (const source of files) {
            const id = "file-" + Array.from(globalThis.crypto.getRandomValues(new Uint8Array(16)), (b2) => b2.toString(16).padStart(2, "0")).join(""), file = { id, name: source.name || "pasted-file", mime: source.type || "application/octet-stream", bytes: source.size };
            put({ ...file, pending: true });
            waiting.set(id, ui.defer(() => {
              waiting.delete(id);
              api.refuse("storage");
              put({ ...file, error: file.name + ": copy receipt missing; remove and select it again before sending" });
            }, 3e4));
            ui.effect("reply.files.read", () => new Promise((resolve, reject) => {
              const r2 = new FileReader();
              ui.listen(r2, "load", () => resolve(r2.result));
              ui.listen(r2, "error", () => reject(Error(file.name + ": file unreadable; select a readable original and retry")));
              r2.readAsDataURL(source);
            }), (result) => {
              if (typeof result !== "string") {
                api.settle(id);
                api.refuse("read");
                put({ ...file, error: result.reason || file.name + ": file read failed; select it again" });
                return;
              }
              ui.send({ a: "reply-file", id, file, data: result.slice(result.indexOf(",") + 1) });
            });
          }
        },
        events: function(add, disabled) {
          return { onPaste: (e2) => {
            if (!disabled && e2.clipboardData?.files.length) {
              e2.preventDefault();
              add([...e2.clipboardData.files]);
            }
          }, onDragOver: (e2) => {
            if (!disabled) e2.preventDefault();
          }, onDrop: (e2) => {
            if (!disabled && e2.dataTransfer?.files.length) {
              e2.preventDefault();
              add([...e2.dataTransfer.files]);
            }
          } };
        },
        controls: function(files, disabled, add, remove) {
          return h2("div", { class: "conversation-files" }, (files || []).map((f2) => h2("span", { key: f2.id, class: "conversation-file" }, f2.name + " \xB7 " + f2.bytes + " bytes" + (f2.pending ? " \xB7 Copying\u2026" : f2.error ? " \xB7 Not attached" : ""), h2("button", { type: "button", disabled, "aria-label": "Remove " + f2.name, onClick: () => remove(f2.id) }, "\xD7"))), h2("label", { class: "attach-label", title: "Attach images or files", "aria-disabled": String(disabled) }, "Attach", h2("input", { type: "file", multiple: true, disabled, "data-reply-files": "pick", "aria-label": "Attach images or files", onChange: (e2) => {
            add([...e2.currentTarget.files]);
            e2.currentTarget.value = "";
          } })));
        },
        markup: function(files, disabled) {
          const esc = (s2) => String(s2).replace(/[&<>"']/g, (c2) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c2]);
          return '<div class="conversation-files">' + (files || []).map((f2) => "<span>" + esc(f2.name) + " \xB7 " + f2.bytes + ' bytes <button type="button" data-qac="remove-file" data-file="' + esc(f2.id) + '" aria-label="Remove ' + esc(f2.name) + '"' + (disabled ? " disabled" : "") + ">\xD7</button></span>").join("") + '<label>Attach <input type="file" multiple data-qac="files" aria-label="Attach images or files"' + (disabled ? " disabled" : "") + "></label></div>";
        }
      });
      module.exports = api;
    }
  });

  // src/answer-card.js
  var require_answer_card = __commonJS({
    "src/answer-card.js"(exports, module) {
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var replyFiles = require_reply_files();
      var { fresh, textProblem, status, step, render, compose, receipt, mount, replay } = execution.module("answer-card", {
        fresh: function(row) {
          return { draft: "", bound: row.revision, op: null, receipt: null };
        },
        textProblem: function(q2, text, files) {
          return replyFiles.problem(files) || (text.trim() || files?.length ? null : "Type an answer");
        },
        status: function(row, local, cap) {
          const q2 = row.question;
          if (local.op && local.receipt && (HELD.includes(local.receipt.state) || local.op.revision === row.revision || !q2)) return local.receipt.state;
          if (!manageable(row, cap) && (!q2 || !q2.parsed || !cap || !cap.answer || !["ask", "review"].includes(row.state))) return "read-only";
          if (local.bound !== row.revision && (local.draft.trim() || local.files?.length)) return "changed";
          return "actionable";
        },
        step: function(row, local, cap, ev, newId) {
          const st = status(row, local, cap), l2 = { ...local };
          const { start } = execution.module("answer-card.step", { start: (payload, revision = row.revision) => {
            l2.op = { id: newId(), task: row.n, revision, ...payload };
            if (payload.text != null && l2.files?.length) l2.op.files = JSON.parse(JSON.stringify(l2.files));
            l2.about = about(row, payload);
            l2.bound = row.revision;
            l2.receipt = { state: "sending" };
            return { local: l2, send: l2.op };
          } });
          switch (ev.type) {
            case "files":
              l2.files = ev.files;
              return { local: l2, send: null };
            case "input":
              if (st !== "changed") l2.bound = row.revision;
              l2.draft = ev.text;
              return { local: l2, send: null };
            case "choose": {
              if (!LIVE.includes(st) || l2.files?.length) return { local, send: null };
              const c2 = row.question.choices.find((x2) => x2.id === ev.id);
              if (!c2 || c2.consequential) return { local, send: null };
              return start({ choice: c2.id });
            }
            case "submit":
              if (!cap || !cap.answer || !LIVE.includes(st) || textProblem(draftQuestion(row, cap), l2.draft, l2.files)) return { local, send: null };
              return start(fallback(row, cap) ? { action: "requeue", text: l2.draft.trim() } : { text: l2.draft.trim() }, st === "changed" && !fallback(row, cap) ? l2.bound : row.revision);
            case "requeue":
              if (!manageable(row, cap) || !LIVE.includes(st) || textProblem(REQUEUE, l2.draft, l2.files)) return { local, send: null };
              return start({ action: "requeue", text: l2.draft.trim() });
            case "later":
            case "close":
              if (!manageable(row, cap) || !LIVE.includes(st)) return { local, send: null };
              return start(ev.type === "later" ? { action: "later" } : { action: "close" });
            case "receipt":
              if (!l2.op || ev.opId !== l2.op.id) return { local, send: null };
              l2.receipt = { state: ["accepted", "rejected", "stale", "unknown", "failed", "partial"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason, next: ev.next, ...ev.note ? { note: true } : {} };
              if (ev.state === "accepted" && (l2.op.text || l2.op.files?.length)) {
                l2.draft = "";
                l2.files = [];
              }
              return { local: l2, send: null };
            case "retry":
              return RETRY.includes(st) ? { local: { ...l2, receipt: { state: "sending" } }, send: l2.op } : { local, send: null };
            default:
              return { local, send: null };
          }
        },
        render: function(board, row, local, cap) {
          const q2 = row.question || { text: row.card || "", parsed: false, choices: [], evidence: [] }, st = status(row, local, cap);
          const id = "qac-" + String(board).replace(/[^\w-]/g, "_") + "-" + row.n, live = LIVE.includes(st), own = !(cap && cap.composer);
          const what = local.op ? "#" + row.n + " " + (local.about || about(row, local.op)) : "";
          let h2 = '<section class="qac" data-qac-state="' + st + '" aria-labelledby="' + id + '-t">';
          h2 += '<p class="qac-head" id="' + id + '-t"><span class="qac-mark" aria-hidden="true">?</span> <a href="' + href(row.url) + '" target="_blank" rel="noopener">#' + esc(row.n) + "</a> " + esc(row.title) + "</p>";
          if (own && cap?.files && st !== "read-only") h2 += replyFiles.markup(local.files, !live);
          if (manageable(row, cap)) {
            h2 += q2.parsed ? lines(q2) + images(q2) : '<pre class="qac-text">' + esc(q2.text || row.card || "No question text supplied") + "</pre>";
            if (own) h2 += q2.parsed ? '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="2" aria-label="Your answer" placeholder="What you did"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(q2, local.draft, local.files) ? "" : " disabled") + ">Send</button>" + hint(id, live && textProblem(q2, local.draft, local.files)) + "</form>" : '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="1" aria-label="Requeue reason" placeholder="Reason to requeue"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(REQUEUE, local.draft, local.files) ? "" : " disabled") + ">Requeue</button>" + hint(id, live && textProblem(REQUEUE, local.draft, local.files)) + "</form>";
            h2 += '<div class="qac-choices">' + (q2.parsed && own ? '<button type="button" data-qac="requeue"' + (live && !textProblem(REQUEUE, local.draft, local.files) ? "" : " disabled") + ">Requeue</button>" : "") + '<button type="button" data-qac="later"' + (live ? "" : " disabled") + '>Later</button><button type="button" data-qac="close"' + (live ? "" : " disabled") + ">Close without publishing</button></div>";
            h2 += receipt(st, local.receipt && local.receipt.reason, row.url, id, local.receipt && local.receipt.next, !!(local.op && local.op.action), what, local.receipt?.note);
            h2 += retry(st, local.receipt && local.receipt.reason);
            return h2 + "</section>";
          }
          if (!q2.parsed) return h2 + '<pre class="qac-text">' + esc(q2.text) + "</pre>" + receipt(st, st === "read-only" ? "This card has no answer form here" : local.receipt && local.receipt.reason, row.url, id, null, false, what) + retry(st, local.receipt && local.receipt.reason) + "</section>";
          h2 += lines(q2) + images(q2);
          if (q2.choices.length) h2 += '<div class="qac-choices" role="group" aria-label="Choices">';
          for (const c2 of q2.choices) {
            const off = !live || c2.consequential || !!local.files?.length, why = c2.consequential ? "Answer on the board: this choice closes or publishes the task, or approves its result" : local.files?.length ? "Send files with a text reply, or remove them to choose an option" : "";
            const name = c2.label == null ? c2.text : c2.label, said = [c2.detail ? id + "-o" + c2.id : "", why ? id + "-c" + c2.id : ""].filter(Boolean).join(" ");
            h2 += '<button type="button" data-qac="choose" data-choice="' + c2.id + '"' + (off ? " disabled" : "") + (said ? ' aria-describedby="' + said + '"' : "") + '><span class="qac-number">' + c2.id + ".</span> <span>" + esc(label(name)) + (c2.recommended ? ' <span class="qac-star" role="img" aria-label="Recommended">\u2605</span>' : "") + "</span></button>";
            for (const m2 of String(name).matchAll(LINK)) h2 += '<a class="qac-link" href="' + href(m2[2]) + '" target="_blank" rel="noopener">' + esc(m2[1]) + " \u2197</a>";
            if (c2.detail) h2 += '<span class="qac-detail" id="' + id + "-o" + c2.id + '">' + linked(c2.detail) + "</span>";
            if (why) h2 += '<span class="qac-why" id="' + id + "-c" + c2.id + '">' + esc(why) + ' <a href="' + href(row.url) + '" target="_blank" rel="noopener">board \u2197</a></span>';
          }
          if (q2.choices.length) h2 += "</div>";
          if (row.pick) h2 += '<p class="qac-pick"><span class="qac-muted">PM</span> ' + esc(row.n) + "." + esc(row.pick.id) + ": " + esc(row.pick.reason) + "</p>";
          if (own && (st !== "read-only" || local.draft)) {
            const bad = textProblem(q2, local.draft, local.files);
            h2 += '<details class="qac-other"' + (local.draft || local.files?.length || !q2.choices.length ? " open" : "") + "><summary>" + (q2.choices.length ? "Other answer" : "Answer") + '</summary><form class="qac-text-form" data-qac="submit"><textarea id="' + id + '-d" data-qac="input" rows="2" aria-label="Your answer" placeholder="' + (q2.choices.length ? "Or type an answer" : "What you did") + '"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !bad ? "" : " disabled") + ">Send</button>" + hint(id, live && bad) + "</form></details>";
          }
          const reason = st === "read-only" ? cap && cap.reason || "Answering is not available here" : local.receipt && local.receipt.reason;
          h2 += receipt(st, reason, row.url, id, local.receipt && local.receipt.next, false, what, local.receipt?.note);
          h2 += retry(st, local.receipt && local.receipt.reason);
          const links = q2.evidence.filter((e2) => !e2.image);
          if (links.length) h2 += '<details class="qac-context"><summary>Evidence</summary><p class="qac-evidence">' + links.map((e2) => '<a href="' + href(e2.url) + '" target="_blank" rel="noopener">' + esc(e2.label) + "</a>").join(" \xB7 ") + "</p></details>";
          return h2 + "</section>";
        },
        compose: function(row, local, cap) {
          const q2 = row.question || {}, st = status(row, local, cap), text = !!(cap && cap.answer) && (fallback(row, cap) || !!q2.parsed);
          if (!text) return null;
          const live = LIVE.includes(st);
          const refusal = local.receipt && ["rejected", "failed", "stale", "partial", "unknown"].includes(st) ? [local.receipt.reason, local.receipt.next].filter(Boolean).join(" \xB7 ") : "";
          return {
            draft: local.draft,
            files: local.files || [],
            caret: local.caret,
            state: st,
            live,
            reason: refusal,
            problem: live ? textProblem(draftQuestion(row, cap), local.draft, local.files) : LABELS[st],
            send: fallback(row, cap) ? "Requeue" : "Send",
            choices: !!(q2.choices && q2.choices.length),
            requeue: manageable(row, cap) && !fallback(row, cap) ? live ? textProblem(REQUEUE, local.draft, local.files) || "" : LABELS[st] : null
          };
        },
        receipt: function(st, reason, url, id, next, action, what, note) {
          if (st === "actionable") return '<p class="qac-receipt" id="' + id + '-r" role="status" aria-live="polite">' + esc(reason || "") + "</p>";
          return '<p class="qac-receipt qac-' + st + '" id="' + id + '-r" role="status" aria-live="polite">' + (what && st !== "read-only" ? '<span class="qac-what">' + esc(what) + "</span> \xB7 " : "") + esc(st === "accepted" && note ? "Task note saved on the board" : st === "accepted" && action ? "Action accepted on the board" : LABELS[st]) + (reason ? ": " + esc(plain(reason)) : "") + (next ? ". " + esc(next) : "") + (["read-only", "unknown", "partial"].includes(st) ? ' <a href="' + href(url) + '" target="_blank" rel="noopener">Open on board \u2197</a>' : "") + "</p>";
        },
        mount: function(el, adapter) {
          const { apply, paint, markup } = execution.module("answer-card.mount", {
            markup: function() {
              return [...cards].map(([k2, c2]) => '<div data-qac-key="' + esc(k2) + '">' + render(c2.board, c2.row, locals.get(k2), adapter.capability(c2.board, c2.row)) + "</div>").join("");
            },
            apply: function(k2, ev) {
              const { board, row } = cards.get(k2), cap = adapter.capability(board, row), r2 = step(row, locals.get(k2), cap, ev, newId);
              locals.set(k2, r2.local);
              adapter.save?.(board, row, r2.local);
              if (ev.type !== "input") paint();
              if (r2.send) ui.effect("answer.send", () => adapter.send(board, r2.send), (x2) => {
                if (cards.has(k2)) apply(k2, { type: "receipt", opId: r2.send.id, ...x2 });
              });
              else if (ev.type === "input") {
                const s2 = el.querySelector('[data-qac-key="' + CSS.escape(k2) + '"] button[type=submit]'), cap2 = adapter.capability(board, row);
                const live = LIVE.includes(status(row, r2.local, cap2)), bad = textProblem(draftQuestion(row, cap2), r2.local.draft, r2.local.files);
                if (s2) s2.disabled = !!bad || !live;
                const rq = el.querySelector('[data-qac-key="' + CSS.escape(k2) + '"] [data-qac=requeue]');
                if (rq) rq.disabled = !live || !!textProblem(REQUEUE, r2.local.draft, r2.local.files);
                const h2 = s2 && s2.parentNode.querySelector(".qac-hint");
                if (h2) h2.textContent = live && bad || "";
                painted = markup();
              }
            },
            paint: function() {
              const a2 = document.activeElement, focus = a2 && el.contains(a2) ? { k: a2.closest("[data-qac-key]")?.dataset.qacKey, sel: a2.dataset.qac === "input" ? "textarea" : a2.dataset.choice ? '[data-choice="' + a2.dataset.choice + '"]' : '[data-qac="' + a2.dataset.qac + '"]', s: a2.selectionStart, e: a2.selectionEnd } : null;
              const scroll = el.scrollTop, open = new Set([...el.querySelectorAll("details[open]")].map((d2) => d2.closest("[data-qac-key]")?.dataset.qacKey + " " + d2.className));
              const html = markup();
              if (html === painted) return;
              el.innerHTML = painted = html;
              for (const d2 of el.querySelectorAll("details")) if (open.has(d2.closest("[data-qac-key]").dataset.qacKey + " " + d2.className)) d2.open = true;
              el.scrollTop = scroll;
              for (const [k2, local] of locals) if (Array.isArray(local.caret) && local.caret.length === 2) {
                const input = el.querySelector('[data-qac-key="' + CSS.escape(k2) + '"] textarea');
                input?.setSelectionRange(...local.caret);
              }
              const t2 = focus && focus.k && el.querySelector('[data-qac-key="' + CSS.escape(focus.k) + '"] ' + focus.sel);
              if (t2) {
                t2.focus();
                if (focus.s != null && t2.setSelectionRange) t2.setSelectionRange(focus.s, focus.e);
              }
            }
          });
          const locals = /* @__PURE__ */ new Map(), cards = /* @__PURE__ */ new Map();
          let painted = null;
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "qac-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e2) => {
            const b2 = e2.target.closest("button[data-qac]"), k2 = b2 && b2.closest("[data-qac-key]")?.dataset.qacKey;
            if (k2 && !b2.disabled) apply(k2, b2.dataset.qac === "choose" ? { type: "choose", id: Number(b2.dataset.choice) } : b2.dataset.qac === "remove-file" ? { type: "files", files: (locals.get(k2).files || []).filter((f2) => f2.id !== b2.dataset.file) } : { type: b2.dataset.qac });
          });
          const add = (k2, files) => {
            if (!k2 || !adapter.capability(cards.get(k2).board, cards.get(k2).row).files || !LIVE.includes(status(cards.get(k2).row, locals.get(k2), adapter.capability(cards.get(k2).board, cards.get(k2).row)))) return;
            replyFiles.read(files, (file) => {
              if (cards.has(k2) && (file.pending || locals.get(k2).files?.some((f2) => f2.id === file.id))) apply(k2, { type: "files", files: [...(locals.get(k2).files || []).filter((f2) => f2.id !== file.id), file] });
            });
          };
          ui.listen(el, "change", (e2) => {
            if (e2.target.dataset.qac === "files") {
              add(e2.target.closest("[data-qac-key]")?.dataset.qacKey, [...e2.target.files]);
            }
          });
          ui.listen(el, "paste", (e2) => {
            if (e2.clipboardData?.files.length) {
              e2.preventDefault();
              add(e2.target.closest("[data-qac-key]")?.dataset.qacKey, [...e2.clipboardData.files]);
            }
          });
          ui.listen(el, "dragover", (e2) => e2.preventDefault());
          ui.listen(el, "drop", (e2) => {
            if (e2.dataTransfer?.files.length) {
              e2.preventDefault();
              add(e2.target.closest("[data-qac-key]")?.dataset.qacKey, [...e2.dataTransfer.files]);
            }
          });
          ui.listen(el, "submit", (e2) => {
            e2.preventDefault();
            const k2 = e2.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k2) apply(k2, { type: "submit" });
          });
          ui.listen(el, "keydown", (e2) => {
            if (e2.target.dataset.qac === "input" && e2.key === "Enter" && !e2.shiftKey && !e2.isComposing && e2.keyCode !== 229) {
              e2.preventDefault();
              const k2 = e2.target.closest("[data-qac-key]")?.dataset.qacKey;
              if (k2) apply(k2, { type: "submit" });
            }
          });
          for (const event of ["select", "keyup", "click"]) ui.listen(el, event, (e2) => {
            const k2 = e2.target.dataset.qac === "input" && e2.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k2 && adapter.save) {
              const c2 = cards.get(k2), local = { ...locals.get(k2), caret: [e2.target.selectionStart, e2.target.selectionEnd] };
              locals.set(k2, local);
              adapter.save(c2.board, c2.row, local);
            }
          });
          ui.listen(el, "input", (e2) => {
            const k2 = e2.target.dataset.qac === "input" && e2.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k2) apply(k2, { type: "input", text: e2.target.value });
          });
          return execution.module("answer-card.mount.controller", {
            files(board, n2, files) {
              const k2 = key(board, { n: n2 });
              if (!cards.has(k2)) return false;
              apply(k2, { type: "files", files });
              return true;
            },
            receiveReceipt(board, opId, reply) {
              for (const [k2, card] of cards) if (card.board === board && locals.get(k2)?.op?.id === opId) {
                apply(k2, { ...reply, type: "receipt", opId });
              }
            },
            // #1662 A3: an adapter whose capability has `composer` owns text entry in its shared composer and drives the card here;
            // text null records only the caret
            input(board, n2, text, caret) {
              const k2 = key(board, { n: n2 });
              if (!cards.has(k2)) return;
              if (text != null && text !== locals.get(k2).draft) apply(k2, { type: "input", text });
              if (caret) {
                const c2 = cards.get(k2), local = { ...locals.get(k2), caret };
                locals.set(k2, local);
                adapter.save?.(c2.board, c2.row, local);
              }
            },
            act(board, n2, type) {
              const k2 = key(board, { n: n2 });
              if (cards.has(k2) && ["submit", "requeue"].includes(type)) apply(k2, { type });
            },
            // rows: [{board, row}] of open questions; a row that left keeps no state
            update(rows) {
              cards.clear();
              for (const c2 of rows) {
                const k2 = key(c2.board, c2.row);
                cards.set(k2, c2);
                if (!locals.has(k2)) locals.set(k2, adapter.load?.(c2.board, c2.row) || fresh(c2.row));
              }
              for (const k2 of [...locals.keys()]) if (!cards.has(k2)) locals.delete(k2);
              paint();
            }
          });
        },
        replay: function(fx, capability) {
          let row = fx.rows[0], local = fresh(row), n2 = 0;
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
            const r2 = step(row, local, capability(fx.board, row), ev.type === "receipt" ? { ...ev, opId: sent[sent.length - 1] && sent[sent.length - 1].id } : ev, () => "op-" + ++n2);
            local = r2.local;
            if (r2.send) sent.push(r2.send);
          }
          return out;
        }
      });
      var root = globalThis;
      var STATES = ["actionable", "changed", "sending", "accepted", "stale", "rejected", "failed", "partial", "unknown", "read-only"];
      var LABELS = {
        actionable: "Waiting for your answer",
        sending: "Sending\u2026",
        accepted: "Answer accepted on the board",
        changed: "Question updated; Send saves your draft as a task note",
        stale: "Question updated; action not sent",
        rejected: "Answer not accepted",
        failed: "Not sent: nothing reached the board",
        partial: "Partly applied: check the board",
        unknown: "Delivery unknown: check the board",
        "read-only": "Read only"
      };
      var LIVE = ["actionable", "changed", "rejected", "failed", "stale"];
      var HELD = ["sending", "unknown", "failed", "partial"];
      var RETRY = ["unknown", "failed", "partial"];
      var REQUEUE = { limits: { lines: 1, chars: 232 } };
      var { esc } = execution.module("answer-card", { esc: (s2) => String(s2 == null ? "" : s2).replace(/[&<>"']/g, (c2) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c2]) });
      var { key } = execution.module("answer-card", { key: (board, row) => board + "#" + row.n });
      var { href } = execution.module("answer-card", { href: (u2) => /^https?:\/\//i.test(u2 || "") ? esc(u2) : "#" });
      var LINK = /\[([^\]]*)\]\(([^)\s]*)\)/g;
      var { label } = execution.module("answer-card", { label: (s2) => String(s2 == null ? "" : s2).replace(LINK, "$1") });
      var { linked } = execution.module("answer-card", { linked: (s2) => {
        const p2 = String(s2 == null ? "" : s2).split(LINK);
        let h2 = esc(p2[0]);
        for (let i2 = 1; i2 < p2.length; i2 += 3) h2 += '<a href="' + href(p2[i2 + 1]) + '" target="_blank" rel="noopener">' + esc(p2[i2]) + "</a>" + esc(p2[i2 + 2]);
        return h2;
      } });
      var { images } = execution.module("answer-card", { images: (q2) => {
        const im = (q2.evidence || []).filter((e2) => e2.image), n2 = (q2.choices || []).length, each = n2 >= 2 && im.length === n2;
        return im.length ? '<p class="qac-images">' + im.map((e2, i2) => (each ? '<span class="qac-shot">' : "") + '<a href="' + href(e2.url) + '" target="_blank" rel="noopener" title="Open ' + esc(e2.label) + ' full size">' + (/^data:image\/(png|jpeg|gif|webp);base64,/.test(e2.src || "") ? '<img class="qac-thumb" src="' + esc(e2.src) + '" alt="' + esc(e2.label) + '">' : esc(e2.label) + " \u2197") + "</a>" + (each ? '<span class="qac-caption">' + (i2 + 1) + ".</span></span>" : "")).join("") + "</p>" : "";
      } });
      var { lines } = execution.module("answer-card", { lines: (q2) => (q2.done ? '<p class="qac-done"><span class="qac-icon" role="img" aria-label="Done">\u2713</span> ' + linked(q2.done) + "</p>" : "") + '<p class="qac-problem"><span class="qac-icon" role="img" aria-label="Problem">\u2691</span> ' + linked(q2.problem) + "</p>" });
      var { manageable } = execution.module("answer-card", { manageable: (row, cap) => row.state === "ask" && cap && cap.answer && cap.manage && !(row.question && row.question.choices && row.question.choices.length) });
      var { fallback } = execution.module("answer-card", { fallback: (row, cap) => manageable(row, cap) && !(row.question && row.question.parsed) });
      var { about } = execution.module("answer-card", { about: (row, op) => {
        if (op.action) return { requeue: "requeue", later: "later", close: "close without publication" }[op.action] || op.action;
        if (op.text != null) return "text answer";
        const c2 = row.question && row.question.choices && row.question.choices.find((x2) => x2.id === op.choice);
        return "answer " + op.choice + (c2 ? ". " + label(c2.label == null ? c2.text : c2.label) : "");
      } });
      var CONFLICT = "Correlation id is already pending for a different request";
      var { retry, plain } = execution.module("answer-card", {
        retry: (st, reason) => RETRY.includes(st) ? '<button type="button" data-qac="retry">' + (st === "failed" && reason !== CONFLICT ? "Send again" : "Check delivery") + "</button>" : "",
        plain: (reason) => reason === CONFLICT ? "An earlier answer with this operation is still pending with different content; Check delivery reads its outcome" : reason
      });
      var { hint } = execution.module("answer-card", { hint: (id, why) => '<span class="qac-hint" id="' + id + '-h" aria-live="polite">' + esc(why || "") + "</span>" });
      var { draftQuestion } = execution.module("answer-card", { draftQuestion: (row, cap) => fallback(row, cap) ? REQUEUE : row.question });
      var api = { STATES, fresh, status, step, render, compose, mount, replay, textProblem };
      module.exports = api;
    }
  });
  return require_answer_card();
})();
