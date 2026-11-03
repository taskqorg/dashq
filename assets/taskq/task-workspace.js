"use strict";
var TaskqTaskWorkspace = (() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __esm = (fn, res, err) => function __init() {
    if (err) throw err[0];
    try {
      return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
    } catch (e4) {
      throw err = [e4], e4;
    }
  };
  var __commonJS = (cb, mod) => function __require() {
    try {
      return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
    } catch (e4) {
      throw mod = 0, e4;
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
            } catch (_5) {
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
              id: () => Array.from(root.crypto.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join("")
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
      var events2 = /* @__PURE__ */ new WeakMap();
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
          if (event && typeof event === "object") events2.set(event, execution.bind((fn) => fn()));
          return Reflect.apply(method, this, [event]);
        });
        const receive = function(event) {
          const inherited = event && events2.get(event);
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
  function _(n3) {
    n3 && n3.parentNode && n3.remove();
  }
  function g(n3, t4, i4) {
    var r4, u4, f4, o4 = {}, e4 = arguments.length;
    for (f4 in t4) "key" == f4 ? r4 = t4[f4] : "ref" == f4 && "function" != typeof n3 ? u4 = t4[f4] : o4[f4] = t4[f4];
    return e4 > 2 && (o4.children = e4 > 3 ? d.call(arguments, 2) : i4), b(n3, o4, r4, u4, null);
  }
  function k(n3, t4, i4) {
    var r4, u4, f4, o4 = w({}, n3.props), e4 = arguments.length;
    for (f4 in t4) "key" == f4 ? r4 = t4[f4] : "ref" == f4 && "function" != typeof n3.type ? u4 = t4[f4] : o4[f4] = t4[f4];
    return e4 > 2 && (o4.children = e4 > 3 ? d.call(arguments, 2) : i4), b(n3.type, o4, void 0 !== r4 ? r4 : n3.key, void 0 !== u4 ? u4 : n3.ref, null);
  }
  function b(i4, r4, u4, f4, o4) {
    var e4 = { type: i4, props: r4, key: u4, ref: f4, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: o4 || ++t, __i: -1, __u: 0 };
    return !o4 && n.vnode && n.vnode(e4), e4;
  }
  function m() {
    return { current: null };
  }
  function M(n3) {
    return n3.children;
  }
  function $(n3, t4) {
    this.props = n3, this.context = t4, this.__g = 0;
  }
  function x(n3, t4) {
    if (null == t4) return n3.__ ? x(n3.__, n3.__i + 1) : null;
    for (var i4; t4 < n3.__k.length; t4++) if ((i4 = n3.__k[t4]) && i4.__e) return i4.__e;
    return "function" != typeof n3.type || n3.props.__P ? null : x(n3);
  }
  function S(n3) {
    if ((n3 = n3.__) && n3.__c && !n3.props.__P) return n3.__e = null, n3.__k.some(function(t4) {
      return t4 && (n3.__e = t4.__e);
    }), S(n3);
  }
  function C(t4) {
    (8 & t4.__g || !(t4.__g |= 8) || !r.push(t4) || f++) && u == n.debounceRendering || ((u = n.debounceRendering) || queueMicrotask)(j);
  }
  function j() {
    var t4, i4, u4, e4, l5, c4, a4, s4, h5;
    try {
      for (i4 = 1; r.length; ) r.length > i4 && r.sort(o), t4 = r.shift(), i4 = r.length, 8 & t4.__g && (e4 = void 0, l5 = void 0, c4 = (l5 = (u4 = t4).__v).__e, a4 = [], s4 = [], (h5 = u4.__P) && ((e4 = w({ constructor: void 0 }, l5)).__v = l5.__v + 1, n.vnode && n.vnode(e4), N(h5, e4, l5, u4.__n, h5.namespaceURI, 32 & l5.__u ? [c4] : null, a4, c4 || x(l5), 32 & l5.__u, s4), e4.__v = l5.__v, e4.__.__k[e4.__i] = e4, z(a4, e4, s4), l5.__ = l5.__e = null, e4.__e != c4 && S(e4)));
    } finally {
      r.length = f = 0;
    }
  }
  function L(n3, t4, i4, r4, u4, f4, o4, e4, l5, c4, a4) {
    var s4, v4, y5, d5, w5, _5, g5 = r4.__k || p, k5 = t4.length;
    for (l5 = H(i4, t4, g5, l5, k5), s4 = 0; s4 < k5; s4++) null != (y5 = i4.__k[s4]) && (v4 = ~y5.__i && g5[y5.__i] || h, y5.__i = s4, _5 = N(n3, y5, v4, u4, f4, o4, e4, l5, c4, a4), d5 = y5.__e, (v4.ref != y5.ref || 8 & v4.__u) && (v4.ref && D(v4.ref, null, y5, v4), y5.ref && a4.push(y5.ref, y5.__c || d5, y5)), w5 = w5 || d5, 4 & y5.__u ? (l5 = I(y5, l5, n3, !v4.__v), v4.__e && (v4.__e = null)) : "function" == typeof y5.type && void 0 !== _5 ? l5 = _5 : d5 && (l5 = d5.nextSibling), y5.__u &= -7);
    return i4.__e = w5, l5;
  }
  function H(n3, t4, i4, r4, u4) {
    var f4, o4, e4, l5, c4, a4, s4, h5, p5, v4, d5 = i4.length, w5 = d5, _5 = 0, g5 = false, k5 = n3.__k = Array(u4);
    for (f4 = 0; f4 < u4; f4++) null != (o4 = t4[f4]) && "boolean" != typeof o4 && "function" != typeof o4 ? ("object" != typeof o4 || o4.constructor == String ? o4 = k5[f4] = b(null, o4) : y(o4) ? o4 = k5[f4] = b(M, { children: o4 }) : void 0 === o4.constructor && o4.__b ? o4 = k5[f4] = b(o4.type, o4.props, o4.key, o4.ref, o4.__v) : k5[f4] = o4, l5 = f4 + _5, o4.__ = n3, o4.__b = n3.__b + 1, e4 = null, ~(c4 = o4.__i = O(o4, i4, l5, w5)) && (w5--, (e4 = i4[c4]) && (e4.__u |= 2)), e4 && e4.__v ? (o4.__u |= 2, c4 == l5 - 1 ? _5-- : c4 == l5 + 1 ? _5++ : c4 != l5 && (c4 > l5 ? _5-- : _5++, g5 = true)) : (~c4 || (u4 > d5 ? _5-- : u4 < d5 && _5++), "function" != typeof o4.type && (o4.__u |= 4))) : k5[f4] = null;
    if (g5) {
      for (a4 = [], s4 = [], f4 = 0; f4 < u4; f4++) if ((o4 = k5[f4]) && 2 & o4.__u) {
        for (h5 = 0, p5 = a4.length; h5 < p5; ) a4[v4 = h5 + p5 >> 1] < o4.__i ? h5 = v4 + 1 : p5 = v4;
        a4[h5] = o4.__i, s4[f4] = h5 + 1;
      }
      for (_5 = a4.length; f4--; ) s4[f4] && (s4[f4] == _5 ? _5-- : k5[f4].__u |= 4);
    }
    if (w5) for (f4 = 0; f4 < d5; f4++) !(e4 = i4[f4]) || 2 & e4.__u || (e4.__e == r4 && (r4 = x(e4)), E(e4, e4));
    return r4;
  }
  function I(n3, t4, i4, r4) {
    var u4, f4, o4;
    if ("function" == typeof n3.type) {
      if (n3.props.__P) return t4;
      if (u4 = n3.__k) for (f4 = 0; f4 < u4.length; f4++) u4[f4] && (u4[f4].__ = n3, t4 = I(u4[f4], t4, i4, false));
      return t4;
    }
    for (t4 && !t4.parentNode && (t4 = x(n3)) && !t4.parentNode && (t4 = null), o4 = t4; o4 && 8 == o4.nodeType; ) o4 = o4.nextSibling;
    for (n3.__e != o4 && (!r4 && i4.moveBefore && n3.__e.parentNode ? i4.moveBefore(n3.__e, t4) : i4.insertBefore(n3.__e, t4 || null)), t4 = n3.__e; (t4 = t4 && t4.nextSibling) && 8 == t4.nodeType; ) ;
    return t4;
  }
  function A(n3, t4) {
    return t4 = t4 || [], null != n3 && "boolean" != typeof n3 && (y(n3) ? n3.some(function(n4) {
      A(n4, t4);
    }) : t4.push(n3)), t4;
  }
  function O(n3, t4, i4, r4) {
    var u4, f4, o4, e4 = n3.key, l5 = n3.type, c4 = t4[i4], a4 = c4 && !(2 & c4.__u);
    if (null === c4 && null == e4 || a4 && e4 == c4.key && l5 == c4.type) return i4;
    if (r4 > (a4 ? 1 : 0)) {
      for (u4 = i4 - 1, f4 = i4 + 1; u4 >= 0 || f4 < t4.length; ) if ((c4 = t4[o4 = u4 >= 0 ? u4-- : f4++]) && !(2 & c4.__u) && e4 == c4.key && l5 == c4.type) return o4;
    }
    return -1;
  }
  function P(n3, t4, i4) {
    null == i4 && (i4 = ""), "-" == t4[0] ? n3.setProperty(t4, i4) : n3[t4] = i4;
  }
  function T(n3, t4, i4, r4, u4) {
    var f4, o4;
    n: if ("style" == t4) if ("string" == typeof i4) n3.style.cssText = i4;
    else {
      if ("string" == typeof r4 && (n3.style.cssText = r4 = ""), r4) for (t4 in r4) i4 && t4 in i4 || P(n3.style, t4, "");
      if (i4) for (t4 in i4) r4 && i4[t4] == r4[t4] || P(n3.style, t4, i4[t4]);
    }
    else if ("o" == t4[0] && "n" == t4[1]) (n3.__e || (n3.__e = {}))[t4] = i4, i4 && r4 || (o4 = a[t4] || (a[t4] = q(t4)), (n3.__a || (n3.__a = {}))[t4] = c, f4 = t4 != (t4 = t4.replace(l, "$1")), (t4 = t4.slice(2))[0] < "a" && (t4 = t4.toLowerCase()), i4 ? n3.addEventListener(t4, o4, f4) : n3.removeEventListener(t4, o4, f4));
    else {
      if ("http://www.w3.org/2000/svg" == u4) t4 = t4.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
      else if ("width" != t4 && "height" != t4 && "href" != t4 && "list" != t4 && "form" != t4 && "tabIndex" != t4 && "download" != t4 && "rowSpan" != t4 && "colSpan" != t4 && "role" != t4 && "popover" != t4 && t4 in n3) try {
        n3[t4] = null == i4 ? "" : i4;
        break n;
      } catch (n4) {
      }
      "function" == typeof i4 || (null == i4 || false === i4 && "-" != t4[4] ? n3.removeAttribute(t4) : n3.setAttribute(t4, "popover" == t4 && 1 == i4 ? "" : i4));
    }
  }
  function q(t4) {
    return function(i4) {
      if (this.__e) {
        var r4 = this.__e[t4];
        if (null == i4[e]) i4[e] = c++;
        else if (i4[e] < this.__a[t4]) return;
        return r4(n.event ? n.event(i4) : i4);
      }
    };
  }
  function N(t4, i4, r4, u4, f4, o4, e4, l5, c4, a4) {
    var s4, h5, v4, d5, g5, k5, b5, m5, S3, C4, j4, H2, I2, A4, O4, P3, T5, q4, N2, z3, D3 = i4.type;
    if (void 0 !== i4.constructor) return null;
    if (128 & r4.__u && (c4 = 32 & r4.__u, s4 = r4.__c.__z)) {
      if (i4.__u |= c4, h5 = o4 = [], 8 == s4.nodeType) for (v4 = 1, d5 = s4.nextSibling; d5; d5 = d5.nextSibling) {
        if (8 == d5.nodeType) {
          if (d5.data.startsWith("$s")) v4++;
          else if (d5.data.startsWith("/$s") && !--v4) break;
        }
        o4.push(d5);
      }
      else o4.push(s4);
      l5 = o4[0];
    }
    (s4 = n.__b) && s4(i4);
    n: if ("function" == typeof D3) {
      g5 = e4.length;
      try {
        if (C4 = i4.props, j4 = (s4 = D3.prototype) && s4.render, H2 = (s4 = D3.contextType) && u4[s4.__c], I2 = s4 ? H2 ? H2.props.value : s4.__ : u4, r4.__c ? 2 & (k5 = i4.__c = r4.__c).__g && (k5.__g |= 1) : (j4 ? i4.__c = k5 = new D3(C4, I2) : (i4.__c = k5 = new $(C4, I2), k5.constructor = D3, k5.render = F), H2 && H2.sub(k5), k5.state || (k5.state = {}), k5.__n = u4, k5.__g |= 8, k5.__h = [], k5.__k = []), j4 && (k5.__s || (k5.__s = k5.state), D3.getDerivedStateFromProps && (k5.__s == k5.state && (k5.__s = w({}, k5.__s)), w(k5.__s, D3.getDerivedStateFromProps(C4, k5.__s)))), b5 = k5.props, m5 = k5.state, k5.__v = i4, r4.__c) {
          if (j4 && !D3.getDerivedStateFromProps && C4 !== b5 && k5.componentWillReceiveProps && k5.componentWillReceiveProps(C4, I2), i4.__v == r4.__v && !(8 & k5.__g) || !(4 & k5.__g) && k5.shouldComponentUpdate && false === k5.shouldComponentUpdate(C4, k5.__s, I2)) {
            i4.__v != r4.__v && (k5.props = C4, k5.state = k5.__s, k5.__g &= -9), i4.__e = r4.__e, i4.__k = r4.__k, i4.__k.some(function(n3) {
              n3 && (n3.__ = i4);
            }), p.push.apply(k5.__h, k5.__k), k5.__k = [], k5.__h.length && e4.push(k5), l5 = x(r4);
            break n;
          }
          k5.componentWillUpdate && k5.componentWillUpdate(C4, k5.__s, I2), j4 && k5.componentDidUpdate && k5.__h.push(function() {
            k5.componentDidUpdate(b5, m5, S3);
          });
        } else j4 && !D3.getDerivedStateFromProps && k5.componentWillMount && k5.componentWillMount(), j4 && k5.componentDidMount && k5.__h.push(k5.componentDidMount);
        if (k5.context = I2, k5.props = C4, k5.__P = t4, k5.__g &= -5, A4 = n.__r, O4 = 0, j4) k5.state = k5.__s, k5.__g &= -9, A4 && A4(i4), s4 = k5.render(k5.props, k5.state, k5.context), p.push.apply(k5.__h, k5.__k), k5.__k = [];
        else do {
          k5.__g &= -9, A4 && A4(i4), s4 = k5.render(k5.props, k5.state, k5.context), k5.state = k5.__s;
        } while (8 & k5.__g && ++O4 < 25);
        k5.state = k5.__s, k5.getChildContext && (u4 = w({}, u4, k5.getChildContext())), j4 && r4.__c && k5.getSnapshotBeforeUpdate && (S3 = k5.getSnapshotBeforeUpdate(b5, m5)), P3 = s4 && s4.type === M && null == s4.key ? s4.props.children : s4, C4.__P && (s4 = l5, f4 = (t4 = C4.__P).namespaceURI, c4 = o4 = null, r4.props && r4.props.__P != t4 && (r4.__k.some(function(n3) {
          n3 && E(n3, n3);
        }), r4.__k = null), l5 = r4.__k ? x(r4, 0) : null), l5 = L(t4, y(P3) ? P3 : [P3], i4, r4, u4, f4, o4, e4, l5, c4, a4), C4.__P && (i4.__e = null, l5 = s4), i4.__u &= -161, 128 & r4.__u && (k5.__z = null), h5 && h5.some(_), k5.__h.length && e4.push(k5), 1 & k5.__g && (k5.__g &= -4);
      } catch (t5) {
        if (e4.length = g5, i4.__v = null, c4 || o4) if (t5.then) {
          if (T5 = 0, i4.__u |= c4 ? 160 : 128, o4) {
            for (~(N2 = o4.indexOf(l5 || void 0)) || (N2 = o4.length); (z3 = o4[N2 - 1]) && 8 == z3.nodeType; ) N2--;
            for (; N2 < o4.length; N2++) if (z3 = o4[N2]) {
              if (o4[N2] = null, 8 == z3.nodeType) {
                if (z3.data.startsWith("$s")) T5++ || (q4 = z3);
                else if (T5 && z3.data.startsWith("/$s") && !--T5) {
                  l5 = z3;
                  break;
                }
              } else if (!T5) break;
            }
          }
          if (!q4) {
            for (; l5 && 8 == l5.nodeType && l5.nextSibling; ) l5 = l5.nextSibling;
            q4 = l5;
          }
          i4.__c.__z || (i4.__c.__z = q4), i4.__e = l5;
        } else o4 && o4.some(_);
        else i4.__e = r4.__e;
        i4.__k || (i4.__k = r4.__k || []), t5.then || V(i4), n.__e(t5, i4, r4);
      }
    } else l5 = i4.__e = B(r4.__e, i4, r4, u4, f4, o4, e4, c4, a4, t4);
    return (s4 = n.diffed) && s4(i4), 128 & i4.__u ? void 0 : l5;
  }
  function V(n3) {
    n3 && (n3.__c && (n3.__c.__g |= 4), n3.__k && n3.__k.some(V));
  }
  function z(t4, i4, r4) {
    for (var u4 = 0; u4 < r4.length; ) D(r4[u4++], r4[u4++], r4[u4++]);
    n.__c && n.__c(i4, t4), t4.some(function(i5) {
      try {
        t4 = i5.__h, i5.__h = [], t4.some(function(n3) {
          n3.call(i5);
        });
      } catch (t5) {
        n.__e(t5, i5.__v);
      }
    });
  }
  function B(t4, i4, r4, u4, f4, o4, e4, l5, c4, a4) {
    var s4, p5, w5, g5, k5, b5, m5, M3, $2, S3 = r4.props || h, C4 = i4.props, j4 = i4.type;
    if ("svg" == j4 ? f4 = "http://www.w3.org/2000/svg" : "math" == j4 ? f4 = "http://www.w3.org/1998/Math/MathML" : f4 || (f4 = "http://www.w3.org/1999/xhtml"), o4) {
      for (s4 = 0; s4 < o4.length; s4++) if ((k5 = o4[s4]) && (j4 ? k5.localName == j4 : 3 == k5.nodeType)) {
        t4 = k5, o4[s4] = null;
        break;
      }
    }
    if (!t4) {
      if (M3 = a4.ownerDocument || document, !j4) return M3.createTextNode(C4);
      t4 = M3.createElementNS(f4, j4, C4.is && C4), l5 && (n.__m && n.__m(i4, o4), l5 = false), o4 = null;
    }
    if (j4) {
      if (a4 = "template" == j4 ? t4.content : t4, o4 = "textarea" == j4 && null != C4.defaultValue ? null : o4 && d.call(a4.childNodes), !l5 && o4) for (S3 = {}, s4 = 0; s4 < t4.attributes.length; s4++) S3[(k5 = t4.attributes[s4]).name] = k5.value;
      for (s4 in S3) k5 = S3[s4], "dangerouslySetInnerHTML" == s4 ? w5 = k5 : "children" == s4 || s4 in C4 || "value" == s4 && "defaultValue" in C4 || "checked" == s4 && "defaultChecked" in C4 || T(t4, s4, null, k5, f4);
      for (s4 in $2 = 1 & r4.__u, C4) k5 = C4[s4], "children" == s4 ? g5 = k5 : "dangerouslySetInnerHTML" == s4 ? p5 = k5 : "value" == s4 ? b5 = k5 : "checked" == s4 ? m5 = k5 : l5 && "function" != typeof k5 || !(S3[s4] !== k5 || $2 && null != k5) || T(t4, s4, k5, S3[s4], f4);
      p5 ? (l5 || w5 && (p5.__html == w5.__html || p5.__html == t4.innerHTML) || (t4.innerHTML = p5.__html), i4.__k = []) : (w5 && (t4.textContent = ""), ("foreignObject" == j4 || "http://www.w3.org/1998/Math/MathML" == f4 && v.test(j4)) && (f4 = "http://www.w3.org/1999/xhtml"), L(a4, y(g5) ? g5 : [g5], i4, r4, u4, f4, o4, e4, o4 ? o4[0] : r4.__k && x(r4, 0), l5, c4), o4 && o4.some(_)), l5 && "textarea" != j4 || (s4 = "value", "progress" == j4 && null == b5 ? t4.removeAttribute(s4) : null == b5 || b5 === t4[s4] && ("progress" != j4 || b5) || T(t4, s4, b5, S3[s4], f4), s4 = "checked", null != m5 && m5 != t4[s4] && T(t4, s4, m5, S3[s4], f4));
    } else S3 === C4 || l5 && t4.data == C4 || (t4.data = C4);
    return t4;
  }
  function D(t4, i4, r4, u4) {
    try {
      "function" == typeof t4 ? i4 ? i4.__x = t4(i4) || 1 : u4 && (u4 = u4.__c || u4.__e) && (i4 = u4.__x) && (u4.__x = null, "function" == typeof i4 ? i4() : t4(null)) : t4.current = i4;
    } catch (t5) {
      n.__e(t5, r4);
    }
  }
  function E(t4, i4, r4) {
    var u4, f4;
    if (n.unmount && n.unmount(t4), !(u4 = t4.ref) || u4.current && u4.current != t4.__e || D(u4, null, i4, t4), u4 = t4.__c) {
      if (u4.componentWillUnmount) try {
        u4.componentWillUnmount();
      } catch (t5) {
        n.__e(t5, i4);
      }
      u4.__P = u4.__n = null;
    }
    if (u4 = t4.__k) for (f4 = 0; f4 < u4.length; f4++) u4[f4] && E(u4[f4], i4, "function" != typeof t4.type || r4 && !t4.props.__P);
    (u4 = t4.__e) && (r4 || _(u4), u4.__e && (u4.__e = null)), t4.__e = t4.__c = t4.__ = null;
  }
  function F(n3, t4, i4) {
    return this.constructor(n3, i4);
  }
  function G(t4, i4) {
    var r4, u4, f4, o4;
    n.__ && n.__(t4, i4), 9 == i4.nodeType && (i4 = i4.documentElement), u4 = (r4 = t4 && 32 & t4.__u) ? null : i4.__k, i4.__k = b(M, { children: [t4] }), f4 = [], o4 = [], N(i4, i4.__k, u4 || h, h, i4.namespaceURI, u4 ? null : i4.firstChild ? d.call(i4.childNodes) : null, f4, u4 ? u4.__e : i4.firstChild, r4, o4), z(f4, i4.__k, o4), i4.__k.props.children = null;
  }
  function J(n3, t4) {
    n3 && "object" == typeof n3 && (n3.__u |= 32), G(n3, t4);
  }
  function K(n3) {
    function t4(n4) {
      var i4, r4;
      return this.getChildContext || (i4 = /* @__PURE__ */ new Set(), (r4 = {})[t4.__c] = this, this.getChildContext = function() {
        return r4;
      }, this.shouldComponentUpdate = function(n5) {
        this.props.value != n5.value && i4.forEach(function(n6) {
          n6.__g |= 4, C(n6);
        });
      }, this.sub = function(n5) {
        i4.add(n5);
        var t5 = n5.componentWillUnmount;
        n5.componentWillUnmount = function() {
          i4.delete(n5), t5 && t5.call(n5);
        };
      }), n4.children;
    }
    return t4.__c = "__cC" + s++, t4.__ = n3, t4.Provider = (t4.Consumer = function(n4, t5) {
      return n4.children(t5);
    }).contextType = t4, t4;
  }
  function Q(n3) {
    return n3.children;
  }
  function R(n3, t4) {
    return b(Q, { __P: t4, children: n3 });
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
      n = { __e: function(n3, t4, i4, r4) {
        for (var u4, o4, e4; t4 = t4.__; ) if ((u4 = t4.__c) && !(1 & u4.__g)) {
          u4.__g |= 4;
          try {
            if ((o4 = u4.constructor) && o4.getDerivedStateFromError && (u4.setState(o4.getDerivedStateFromError(n3)), e4 = 8 & u4.__g), u4.componentDidCatch && (u4.componentDidCatch(n3, r4 || {}), e4 = 8 & u4.__g), e4) return void (u4.__g |= 2);
          } catch (t5) {
            n3 = t5, e4 = 0;
          }
        }
        throw f = 0, n3;
      } }, t = 0, i = function(n3) {
        return null != n3 && void 0 === n3.constructor;
      }, $.prototype.setState = function(n3, t4) {
        var i4 = this.__s;
        i4 && i4 != this.state || (i4 = this.__s = w({}, this.state)), "function" == typeof n3 && (n3 = n3(w({}, i4), this.props)), n3 && (w(i4, n3), this.__v && (t4 && this.__k.push(t4), C(this)));
      }, $.prototype.forceUpdate = function(n3) {
        this.__v && (this.__g |= 4, n3 && this.__h.push(n3), C(this));
      }, $.prototype.render = M, r = [], f = 0, o = function(n3, t4) {
        return n3.__v.__b - t4.__v.__b;
      }, e = /* @__PURE__ */ Symbol(), l = /(PointerCapture)$|Capture$/i, c = 0, a = {}, s = 0;
    }
  });

  // node_modules/preact/hooks/dist/hooks.mjs
  var hooks_exports = {};
  __export(hooks_exports, {
    useCallback: () => j2,
    useContext: () => w2,
    useDebugValue: () => x2,
    useEffect: () => A2,
    useErrorBoundary: () => O2,
    useId: () => P2,
    useImperativeHandle: () => q2,
    useLayoutEffect: () => F2,
    useMemo: () => b2,
    useReducer: () => _2,
    useRef: () => T2,
    useState: () => d2
  });
  function y2(n3, t4) {
    a2.__h && a2.__h(r2, n3, f2 || t4), f2 = 0;
    var u4 = r2.__H || (r2.__H = { __: [], __h: [] });
    return n3 >= u4.__.length && u4.__.push({}), u4.__[n3];
  }
  function d2(n3) {
    return f2 = 1, _2(G2, n3);
  }
  function _2(n3, u4, i4) {
    var f4 = y2(t2++, 2);
    if (f4.t = n3, !f4.__c && (f4.__ = [i4 ? i4(u4) : G2(void 0, u4), function(n4) {
      var t4 = f4.__N ? f4.__N[0] : f4.__[0], r4 = f4.t(t4, n4);
      o2(t4, r4) || (f4.__N = [r4, f4.__[1]], f4.__c.setState({}));
    }], f4.__c = r2, !r2.__f)) {
      r2.__f = true;
      var c4 = r2.shouldComponentUpdate;
      r2.shouldComponentUpdate = function(n4, t4, r4) {
        var u5 = this.__H;
        if (!u5) return true;
        var i5 = false, f5 = this.props != n4;
        if (u5.__.some(function(n5) {
          n5.__N && (i5 = true, o2(n5.__[0], n5.__N[0]) || (f5 = true));
        }), c4) {
          var e4 = c4.call(this, n4, t4, r4);
          return i5 ? e4 || f5 : e4;
        }
        return !i5 || f5;
      };
    }
    return f4.__;
  }
  function A2(n3, u4) {
    var i4 = y2(t2++, 3);
    !a2.__s && E2(i4.__H, u4) && (i4.__P = true, i4.__ = n3, i4.u = u4, r2.__H.__h.push(i4));
  }
  function F2(n3, u4) {
    var i4 = y2(t2++, 4);
    !a2.__s && E2(i4.__H, u4) && (i4.__P = false, i4.__ = n3, i4.u = u4, r2.__h.push(i4));
  }
  function T2(n3) {
    return f2 = 5, b2(function() {
      return { current: n3 };
    }, []);
  }
  function q2(n3, t4, r4) {
    f2 = 6, F2(function() {
      if ("function" == typeof n3) {
        var r5 = n3(t4());
        return function() {
          n3(null), r5 && "function" == typeof r5 && r5();
        };
      }
      if (n3) return n3.current = t4(), function() {
        return n3.current = null;
      };
    }, null == r4 ? r4 : r4.concat(n3));
  }
  function b2(n3, r4) {
    var u4 = y2(t2++, 7);
    return E2(u4.__H, r4) && (u4.__ = n3(), u4.__H = r4), u4.__;
  }
  function j2(n3, t4) {
    return f2 = 8, b2(function() {
      return n3;
    }, t4);
  }
  function w2(n3) {
    var u4 = r2.context[n3.__c], i4 = y2(t2++, 9);
    return i4.c = n3, u4 ? (null == i4.__ && (i4.__ = true, u4.sub(r2)), u4.props.value) : n3.__;
  }
  function x2(n3, t4) {
    a2.useDebugValue && a2.useDebugValue(t4 ? t4(n3) : n3);
  }
  function O2(n3) {
    var u4 = y2(t2++, 10), i4 = d2();
    return u4.__ = n3, r2.componentDidCatch || (r2.componentDidCatch = function(n4, t4) {
      u4.__ && u4.__(n4, t4), i4[1](n4);
    }), [i4[0], function() {
      i4[1](void 0);
    }];
  }
  function P2() {
    var n3 = y2(t2++, 11);
    if (!n3.__) {
      for (var u4 = r2.__v; !u4.__m && u4.__; ) u4 = u4.__;
      var i4 = u4.__m || (u4.__m = [0, 0]);
      n3.__ = "P" + i4[0] + "-" + i4[1]++;
    }
    return n3.__;
  }
  function g2() {
    var n3;
    do {
      for (; n3 = e2.shift(); ) try {
        C2(n3);
      } catch (t5) {
        a2.__e(t5, { __: (n3 = n3.__P) && n3.__v });
      }
      for (; n3 = c2.shift(); ) {
        var t4 = n3.__H;
        if (n3.__P && t4) try {
          t4.__h.some(C2), t4.__h.some(D2), t4.__h = [];
        } catch (r4) {
          t4.__h = [], a2.__e(r4, n3.__v);
        }
      }
    } while (e2.length);
  }
  function z2(n3) {
    var t4, r4 = function() {
      clearTimeout(u4), k2 && cancelAnimationFrame(t4), setTimeout(n3);
    }, u4 = setTimeout(r4, 35);
    k2 && (t4 = requestAnimationFrame(r4));
  }
  function B2(n3) {
    1 != n3 && i2 == a2.requestAnimationFrame || ((i2 = a2.requestAnimationFrame) || z2)(g2);
  }
  function C2(n3) {
    var t4 = r2, u4 = n3.__c;
    "function" == typeof u4 && (n3.__c = void 0, u4()), r2 = t4;
  }
  function D2(n3) {
    var t4 = r2;
    n3.__c = n3.__(), r2 = t4;
  }
  function E2(n3, t4) {
    return !n3 || n3.length != t4.length || t4.some(function(t5, r4) {
      return !o2(t5, n3[r4]);
    });
  }
  function G2(n3, t4) {
    return "function" == typeof t4 ? t4(n3) : t4;
  }
  var t2, r2, u2, i2, o2, f2, c2, e2, a2, v2, l2, m2, s2, h2, p2, k2;
  var init_hooks = __esm({
    "node_modules/preact/hooks/dist/hooks.mjs"() {
      init_preact();
      o2 = Object.is;
      f2 = 0;
      c2 = [];
      e2 = [];
      a2 = n;
      v2 = a2.__b;
      l2 = a2.__r;
      m2 = a2.diffed;
      s2 = a2.__c;
      h2 = a2.unmount;
      p2 = a2.__;
      a2.__b = function(n3) {
        r2 = null, v2 && v2(n3);
      }, a2.__ = function(n3, t4) {
        n3 && t4.__k && t4.__k.__m && (n3.__m = t4.__k.__m), p2 && p2(n3, t4);
      }, a2.__r = function(n3) {
        l2 && l2(n3), t2 = 0;
        var i4 = (r2 = n3.__c).__H;
        i4 && (u2 == r2 ? r2.__h = [] : (i4.__h.some(C2), i4.__h.some(D2), t2 = 0), i4.__h = [], i4.__.some(function(n4) {
          n4.__N && (n4.__ = n4.__N), n4.u = n4.__N = void 0;
        })), u2 = r2;
      }, a2.diffed = function(n3) {
        m2 && m2(n3);
        var t4 = n3.__c;
        t4 && t4.__H && (t4.__H.__h.length && B2(c2.push(t4)), t4.__H.__.some(function(n4) {
          n4.u && (n4.__H = n4.u);
        })), u2 = r2 = null;
      }, a2.__c = function(n3, t4) {
        t4.some(function(n4) {
          try {
            n4.__h.some(C2), n4.__h = n4.__h.filter(function(n5) {
              return !n5.__ || D2(n5);
            });
          } catch (r4) {
            t4.some(function(n5) {
              n5.__h && (n5.__h = []);
            }), t4 = [], a2.__e(r4, n4.__v);
          }
        }), s2 && s2(n3, t4);
      }, a2.unmount = function(n3) {
        h2 && h2(n3);
        var t4, r4, u4 = n3.__c;
        u4 && u4.__H && (u4.__H.__.some(function(u5) {
          try {
            if (u5.__P && u5.__c) {
              if (void 0 === r4) {
                for (r4 = n3.__; r4 && (!r4.__c || !r4.__c.__P); ) r4 = r4.__;
                r4 = r4 && r4.__c;
              }
              u5.__P = r4, B2(e2.push(u5));
            } else C2(u5);
          } catch (n4) {
            t4 = n4;
          }
        }), u4.__H = void 0, t4 && a2.__e(t4, u4.__v));
      };
      k2 = "function" == typeof requestAnimationFrame;
    }
  });

  // node_modules/@preact/signals-core/dist/signals-core.module.js
  function t3() {
    if (!(v3 > 1)) {
      var i4, t4 = false;
      !(function() {
        var i5 = c3;
        c3 = void 0;
        while (void 0 !== i5) {
          var t5 = i5.S;
          if (t5.v === i5.v) {
            for (var n4 = t5.t; void 0 !== n4; n4 = n4.x) if (n4.i === i5.i) n4.i = t5.i;
          }
          i5 = i5.o;
        }
      })();
      while (void 0 !== h3) {
        var n3 = h3;
        h3 = void 0;
        s3++;
        while (void 0 !== n3) {
          var r4 = n3.u;
          n3.u = void 0;
          n3.f &= -3;
          if (!(8 & n3.f) && w3(n3)) try {
            n3.c();
          } catch (n4) {
            if (!t4) {
              i4 = n4;
              t4 = true;
            }
          }
          n3 = r4;
        }
      }
      s3 = 0;
      v3--;
      if (t4) throw i4;
    } else v3--;
  }
  function n2(i4) {
    if (v3 > 0) return i4();
    e3 = ++u3;
    v3++;
    try {
      return i4();
    } finally {
      t3();
    }
  }
  function f3(i4) {
    var t4 = o3, n3 = r3;
    o3 = void 0;
    r3 = void 0;
    try {
      return i4();
    } finally {
      o3 = t4;
      r3 = n3;
    }
  }
  function a3(i4) {
    if (void 0 !== o3) {
      var t4 = i4.n;
      if (void 0 === t4 || t4.t !== o3) {
        t4 = { i: 0, S: i4, p: o3.s, n: void 0, t: o3, e: void 0, x: void 0, r: t4 };
        if (void 0 !== o3.s) o3.s.n = t4;
        o3.s = t4;
        i4.n = t4;
        if (32 & o3.f) i4.S(t4);
        return t4;
      } else if (-1 === t4.i) {
        t4.i = 0;
        if (void 0 !== t4.n) {
          t4.n.p = t4.p;
          if (void 0 !== t4.p) t4.p.n = t4.n;
          t4.p = o3.s;
          t4.n = void 0;
          o3.s.n = t4;
          o3.s = t4;
        }
        return t4;
      }
    }
  }
  function l3(i4, t4) {
    this.v = i4;
    this.i = 0;
    this.n = void 0;
    this.t = void 0;
    this.l = 0;
    this.W = null == t4 ? void 0 : t4.watched;
    this.Z = null == t4 ? void 0 : t4.unwatched;
    this.name = null == t4 ? void 0 : t4.name;
  }
  function y3(i4, t4) {
    return new l3(i4, t4);
  }
  function w3(i4) {
    for (var t4 = i4.s; void 0 !== t4; t4 = t4.n) if (t4.S.i !== t4.i || !t4.S.h() || t4.S.i !== t4.i) return true;
    return false;
  }
  function _3(i4) {
    for (var t4 = i4.s; void 0 !== t4; t4 = t4.n) {
      var n3 = t4.S.n;
      if (void 0 !== n3) t4.r = n3;
      t4.S.n = t4;
      t4.i = -1;
      if (void 0 === t4.n) {
        i4.s = t4;
        break;
      }
    }
  }
  function b3(i4) {
    var t4 = i4.s, n3 = void 0;
    while (void 0 !== t4) {
      var r4 = t4.p;
      if (-1 === t4.i) {
        t4.S.U(t4);
        if (void 0 !== r4) r4.n = t4.n;
        if (void 0 !== t4.n) t4.n.p = r4;
      } else n3 = t4;
      t4.S.n = t4.r;
      if (void 0 !== t4.r) t4.r = void 0;
      t4 = r4;
    }
    i4.s = n3;
  }
  function p3(i4, t4) {
    l3.call(this, void 0, t4);
    this.x = i4;
    this.s = void 0;
    this.g = d3 - 1;
    this.f = 4;
  }
  function g3(i4, t4) {
    return new p3(i4, t4);
  }
  function S2(i4) {
    var n3 = i4.m;
    i4.m = void 0;
    if ("function" == typeof n3) {
      v3++;
      var r4 = o3;
      o3 = void 0;
      try {
        n3();
      } catch (t4) {
        i4.f &= -2;
        i4.f |= 8;
        m3(i4);
        throw t4;
      } finally {
        o3 = r4;
        t3();
      }
    }
  }
  function m3(i4) {
    for (var t4 = i4.s; void 0 !== t4; t4 = t4.n) t4.S.U(t4);
    i4.x = void 0;
    i4.s = void 0;
    S2(i4);
  }
  function x3(i4) {
    if (o3 !== this) throw new Error("Out-of-order effect");
    b3(this);
    o3 = i4;
    this.f &= -2;
    if (8 & this.f) m3(this);
    t3();
  }
  function E3(i4, t4) {
    this.x = i4;
    this.m = void 0;
    this.s = void 0;
    this.u = void 0;
    this.f = 32;
    this.name = null == t4 ? void 0 : t4.name;
    if (r3) r3.push(this);
  }
  function j3(i4, t4) {
    var n3 = new E3(i4, t4);
    try {
      n3.c();
    } catch (i5) {
      n3.d();
      throw i5;
    }
    var r4 = n3.d.bind(n3);
    r4[Symbol.dispose] = r4;
    return r4;
  }
  function C3(i4) {
    return function() {
      var t4 = arguments, r4 = this;
      return n2(function() {
        return f3(function() {
          return i4.apply(r4, [].slice.call(t4));
        });
      });
    };
  }
  function O3() {
    var i4 = r3;
    r3 = [];
    return function() {
      var t4 = r3;
      if (r3 && i4) i4 = i4.concat(r3);
      r3 = i4;
      return t4;
    };
  }
  function T3(i4) {
    return function() {
      var t4, n3, o4 = O3();
      try {
        n3 = i4.apply(void 0, [].slice.call(arguments));
      } catch (i5) {
        r3 = void 0;
        throw i5;
      } finally {
        t4 = o4();
      }
      k3(n3);
      n3[Symbol.dispose] = C3(function() {
        if (t4) for (var i5 = 0; i5 < t4.length; i5++) t4[i5].dispose();
        t4 = void 0;
      });
      return n3;
    };
  }
  var i3, r3, o3, h3, v3, s3, u3, e3, c3, d3, k3;
  var init_signals_core_module = __esm({
    "node_modules/@preact/signals-core/dist/signals-core.module.js"() {
      i3 = /* @__PURE__ */ Symbol.for("preact-signals");
      o3 = void 0;
      h3 = void 0;
      v3 = 0;
      s3 = 0;
      u3 = 0;
      e3 = 0;
      c3 = void 0;
      d3 = 0;
      l3.prototype.brand = i3;
      l3.prototype.h = function() {
        return true;
      };
      l3.prototype.S = function(i4) {
        var t4 = this, n3 = this.t;
        if (n3 !== i4 && void 0 === i4.e) {
          i4.x = n3;
          this.t = i4;
          if (void 0 !== n3) n3.e = i4;
          else f3(function() {
            var i5;
            null == (i5 = t4.W) || i5.call(t4);
          });
        }
      };
      l3.prototype.U = function(i4) {
        var t4 = this;
        if (void 0 !== this.t) {
          var n3 = i4.e, r4 = i4.x;
          if (void 0 !== n3) {
            n3.x = r4;
            i4.e = void 0;
          }
          if (void 0 !== r4) {
            r4.e = n3;
            i4.x = void 0;
          }
          if (i4 === this.t) {
            this.t = r4;
            if (void 0 === r4) f3(function() {
              var i5;
              null == (i5 = t4.Z) || i5.call(t4);
            });
          }
        }
      };
      l3.prototype.subscribe = function(i4) {
        var t4 = this;
        return j3(function() {
          var n3 = t4.value;
          f3(function() {
            return i4(n3);
          });
        }, { name: "sub" });
      };
      l3.prototype.valueOf = function() {
        return this.value;
      };
      l3.prototype.toString = function() {
        return this.value + "";
      };
      l3.prototype.toJSON = function() {
        return this.value;
      };
      l3.prototype.peek = function() {
        var i4 = this;
        return f3(function() {
          return i4.value;
        });
      };
      Object.defineProperty(l3.prototype, "value", { get: function() {
        var i4 = a3(this);
        if (void 0 !== i4) i4.i = this.i;
        return this.v;
      }, set: function(i4) {
        if (i4 !== this.v) {
          if (s3 > 100) throw new Error("Cycle detected");
          !(function(i5) {
            if (0 !== v3 && 0 === s3) {
              if (i5.l !== e3) {
                i5.l = e3;
                c3 = { S: i5, v: i5.v, i: i5.i, o: c3 };
              }
            }
          })(this);
          this.v = i4;
          this.i++;
          d3++;
          v3++;
          try {
            for (var n3 = this.t; void 0 !== n3; n3 = n3.x) n3.t.N();
          } finally {
            t3();
          }
        }
      } });
      p3.prototype = new l3();
      p3.prototype.h = function() {
        this.f &= -3;
        if (1 & this.f) return false;
        if (32 == (36 & this.f)) return true;
        this.f &= -5;
        if (this.g === d3) return true;
        this.g = d3;
        this.f |= 1;
        if (this.i > 0 && !w3(this)) {
          this.f &= -2;
          return true;
        }
        var i4 = o3;
        try {
          _3(this);
          o3 = this;
          var t4 = this.x();
          if (16 & this.f || this.v !== t4 || 0 === this.i) {
            this.v = t4;
            this.f &= -17;
            this.i++;
          }
        } catch (i5) {
          this.v = i5;
          this.f |= 16;
          this.i++;
        }
        o3 = i4;
        b3(this);
        this.f &= -2;
        return true;
      };
      p3.prototype.S = function(i4) {
        if (void 0 === this.t) {
          this.f |= 36;
          for (var t4 = this.s; void 0 !== t4; t4 = t4.n) t4.S.S(t4);
        }
        l3.prototype.S.call(this, i4);
      };
      p3.prototype.U = function(i4) {
        if (void 0 !== this.t) {
          l3.prototype.U.call(this, i4);
          if (void 0 === this.t) {
            this.f &= -33;
            for (var t4 = this.s; void 0 !== t4; t4 = t4.n) t4.S.U(t4);
          }
        }
      };
      p3.prototype.N = function() {
        if (!(2 & this.f)) {
          this.f |= 6;
          for (var i4 = this.t; void 0 !== i4; i4 = i4.x) i4.t.N();
        }
      };
      Object.defineProperty(p3.prototype, "value", { get: function() {
        if (1 & this.f) throw new Error("Cycle detected");
        var i4 = a3(this);
        this.h();
        if (void 0 !== i4) i4.i = this.i;
        if (16 & this.f) throw this.v;
        return this.v;
      } });
      E3.prototype.c = function() {
        var i4 = this.S();
        try {
          if (8 & this.f) return;
          if (void 0 === this.x) return;
          var t4 = this.x();
          if ("function" == typeof t4) this.m = t4;
        } finally {
          i4();
        }
      };
      E3.prototype.S = function() {
        if (1 & this.f) throw new Error("Cycle detected");
        this.f |= 1;
        this.f &= -9;
        S2(this);
        _3(this);
        v3++;
        var i4 = o3;
        o3 = this;
        return x3.bind(this, i4);
      };
      E3.prototype.N = function() {
        if (!(2 & this.f)) {
          this.f |= 2;
          this.u = h3;
          h3 = this;
        }
      };
      E3.prototype.d = function() {
        this.f |= 8;
        if (!(1 & this.f)) m3(this);
      };
      E3.prototype.dispose = function() {
        this.d();
      };
      k3 = function(i4) {
        for (var t4 in i4) {
          var n3 = i4[t4];
          if ("function" == typeof n3) i4[t4] = C3(n3);
          else if ("object" == typeof n3 && null !== n3 && !("brand" in n3)) k3(n3);
        }
      };
    }
  });

  // node_modules/@preact/signals/dist/signals.module.js
  var signals_module_exports = {};
  __export(signals_module_exports, {
    Signal: () => l3,
    action: () => C3,
    batch: () => n2,
    computed: () => g3,
    createModel: () => T3,
    effect: () => j3,
    signal: () => y3,
    untracked: () => f3,
    useComputed: () => useComputed,
    useModel: () => M2,
    useSignal: () => useSignal,
    useSignalEffect: () => useSignalEffect
  });
  function g4(i4, r4) {
    n[i4] = r4.bind(null, n[i4] || function() {
    });
  }
  function b4(i4) {
    if (d4) {
      var n3 = d4;
      d4 = void 0;
      n3();
    }
    d4 = i4 && i4.S();
  }
  function y4(i4) {
    var n3 = this, t4 = i4.data, f4 = useSignal(t4);
    f4.name = "ReactiveDom";
    f4.value = t4;
    var e4 = b2(function() {
      var i5 = n3, t5 = n3.__v;
      while (t5 = t5.__) if (t5.__c) {
        t5.__c.__$f |= 4;
        break;
      }
      var o4 = g3(function() {
        var i6 = f4.value.value;
        return 0 === i6 ? 0 : true === i6 ? "" : i6 || "";
      }), e5 = g3(function() {
        return !Array.isArray(o4.value) && !i(o4.value);
      }), a5 = j3(function() {
        this.N = F3;
        if (e5.value) {
          var n4 = o4.value;
          if (i5.__v && i5.__v.__e && 3 === i5.__v.__e.nodeType) i5.__v.__e.data = n4;
        }
      }), v5 = n3.__$u.d;
      n3.__$u.d = function() {
        a5();
        v5.call(this);
      };
      return [e5, o4];
    }, []), a4 = e4[0], v4 = e4[1];
    return a4.value ? v4.peek() : v4.value;
  }
  function w4(i4, n3, r4, t4) {
    var o4 = n3 in i4 && void 0 === i4.ownerSVGElement, f4 = y3(r4);
    return { o: function(i5, n4) {
      f4.value = i5;
      t4 = n4;
    }, d: j3(function() {
      this.N = F3;
      var r5 = f4.value.value;
      if (t4[n3] !== r5) {
        t4[n3] = r5;
        if (o4) i4[n3] = r5;
        else if (null != r5 && (false !== r5 || "-" === n3[4])) i4.setAttribute(n3, r5);
        else i4.removeAttribute(n3);
      }
    }) };
  }
  function useSignal(i4, n3) {
    return b2(function() {
      return y3(i4, n3);
    }, []);
  }
  function useComputed(i4, n3) {
    var r4 = T2(i4);
    r4.current = i4;
    h4.__$f |= 4;
    return b2(function() {
      return g3(function() {
        return r4.current();
      }, n3);
    }, []);
  }
  function A3() {
    n2(function() {
      var i4;
      while (i4 = m4.shift()) l4.call(i4);
    });
  }
  function T4() {
    if (1 === m4.push(this)) (n.requestAnimationFrame || k4)(A3);
  }
  function x4() {
    n2(function() {
      var i4;
      while (i4 = _4.shift()) l4.call(i4);
    });
  }
  function F3() {
    if (1 === _4.push(this)) (n.requestAnimationFrame || q3)(x4);
  }
  function useSignalEffect(i4, n3) {
    var r4 = T2(i4);
    r4.current = i4;
    var t4 = h4;
    A2(function() {
      return j3(function() {
        this.N = T4;
        if (!(t4 && 8 & t4.__$f)) return r4.current();
      }, n3);
    }, []);
  }
  function M2(i4) {
    var n3 = b2(function() {
      return i4();
    }, []);
    A2(function() {
      return n3[Symbol.dispose];
    }, [n3]);
    return n3;
  }
  var l4, h4, d4, p4, m4, _4, k4, q3;
  var init_signals_module = __esm({
    "node_modules/@preact/signals/dist/signals.module.js"() {
      init_preact();
      init_hooks();
      init_signals_core_module();
      init_signals_core_module();
      p4 = "undefined" != typeof window && !!window.__PREACT_SIGNALS_DEVTOOLS__;
      m4 = [];
      _4 = [];
      j3(function() {
        l4 = this.N;
      })();
      y4.displayName = "ReactiveTextNode";
      Object.defineProperties(l3.prototype, { constructor: { configurable: true, value: void 0 }, type: { configurable: true, value: y4 }, props: { configurable: true, get: function() {
        var i4 = this;
        return { data: { get value() {
          return i4.value;
        } } };
      } }, __b: { configurable: true, value: 1 } });
      g4("__b", function(i4, n3) {
        b4();
        h4 = void 0;
        if ("string" == typeof n3.type) {
          var r4, t4 = n3.props;
          for (var o4 in t4) if ("children" !== o4) {
            var f4 = t4[o4];
            if (f4 instanceof l3) {
              if (!r4) n3.__np = r4 = {};
              r4[o4] = f4;
              t4[o4] = f4.peek();
            }
          }
        }
        i4(n3);
      });
      g4("__r", function(i4, n3) {
        i4(n3);
        if (n3.type !== M) {
          b4();
          var r4, o4 = n3.__c;
          if (o4) {
            o4.__$f &= -2;
            if (void 0 === (r4 = o4.__$u)) o4.__$u = r4 = (function(i5, n4) {
              var r5;
              j3(function() {
                r5 = this;
              }, { name: n4 });
              r5.c = i5;
              return r5;
            })(/* @__PURE__ */ (function(i5) {
              return function() {
                var n4;
                if (p4) null == (n4 = this.y) || n4.call(this);
                i5.__$f |= 1;
                i5.setState({});
              };
            })(o4), "function" == typeof n3.type ? n3.type.displayName || n3.type.name : "");
          }
          h4 = o4;
          b4(r4);
        }
      });
      g4("__e", function(i4, n3, r4, t4) {
        b4();
        h4 = void 0;
        i4(n3, r4, t4);
      });
      g4("diffed", function(i4, n3) {
        b4();
        h4 = void 0;
        var r4;
        if ("string" == typeof n3.type && (r4 = n3.__e)) {
          var t4 = n3.__np, o4 = n3.props, f4 = r4.U;
          if (f4) for (var e4 in f4) {
            var u4 = f4[e4];
            if (!(void 0 === u4 || t4 && e4 in t4)) {
              u4.d();
              f4[e4] = void 0;
            }
          }
          if (t4) {
            if (!f4) {
              f4 = {};
              r4.U = f4;
            }
            for (var a4 in t4) {
              var c4 = f4[a4], v4 = t4[a4];
              if (void 0 === c4) {
                c4 = w4(r4, a4, v4, o4);
                f4[a4] = c4;
              } else c4.o(v4, o4);
            }
          }
        }
        i4(n3);
      });
      g4("unmount", function(i4, n3) {
        if ("string" == typeof n3.type) {
          var r4 = n3.__e;
          if (r4) {
            var t4 = r4.U;
            if (t4) {
              r4.U = void 0;
              for (var o4 in t4) {
                var f4 = t4[o4];
                if (f4) f4.d();
              }
            }
          }
          var e4 = n3.__np;
          if (e4) {
            var u4 = n3.props;
            for (var a4 in e4) u4[a4] = e4[a4];
          }
          n3.__np = void 0;
        } else {
          var c4 = n3.__c;
          if (c4) {
            c4.__$f |= 8;
            var v4 = c4.__$u;
            if (v4) {
              c4.__$u = void 0;
              v4.d();
            }
          }
        }
        i4(n3);
      });
      g4("__h", function(i4, n3, r4, t4) {
        if (t4 < 3) n3.__$f |= 2;
        i4(n3, r4, t4);
      });
      $.prototype.shouldComponentUpdate = function(i4, n3) {
        if (this.__R) return true;
        var r4 = this.__$u, t4 = r4 && void 0 !== r4.s;
        for (var o4 in n3) return true;
        if (this.__f || "boolean" == typeof this.u && true === this.u) {
          var f4 = 2 & this.__$f;
          if (!(t4 || f4 || 4 & this.__$f)) return true;
          if (1 & this.__$f) return true;
        } else {
          if (!(t4 || 4 & this.__$f)) return true;
          if (3 & this.__$f) return true;
        }
        for (var e4 in i4) if ("__source" !== e4 && i4[e4] !== this.props[e4]) return true;
        for (var u4 in this.props) if (!(u4 in i4)) return true;
        return false;
      };
      k4 = "undefined" == typeof requestAnimationFrame ? setTimeout : function(i4) {
        var n3 = function() {
          clearTimeout(r4);
          cancelAnimationFrame(t4);
          i4();
        }, r4 = setTimeout(n3, 35), t4 = requestAnimationFrame(n3);
      };
      q3 = function(i4) {
        queueMicrotask(function() {
          queueMicrotask(i4);
        });
      };
    }
  });

  // node_modules/regexparam/dist/index.mjs
  function parse(input, loose) {
    if (input instanceof RegExp) return { keys: false, pattern: input };
    var c4, o4, tmp, ext, keys = [], pattern = "", arr = input.split("/");
    arr[0] || arr.shift();
    while (tmp = arr.shift()) {
      c4 = tmp[0];
      if (c4 === "*") {
        keys.push(c4);
        pattern += tmp[1] === "?" ? "(?:/(.*))?" : "/(.*)";
      } else if (c4 === ":") {
        o4 = tmp.indexOf("?", 1);
        ext = tmp.indexOf(".", 1);
        keys.push(tmp.substring(1, !!~o4 ? o4 : !!~ext ? ext : tmp.length));
        pattern += !!~o4 && !~ext ? "(?:/([^/]+?))?" : "/([^/]+?)";
        if (!!~ext) pattern += (!!~o4 ? "?" : "") + "\\" + tmp.substring(ext);
      } else {
        pattern += "/" + tmp;
      }
    }
    return {
      keys,
      pattern: new RegExp("^" + pattern + (loose ? "(?=$|/)" : "/?$"), "i")
    };
  }
  var init_dist = __esm({
    "node_modules/regexparam/dist/index.mjs"() {
    }
  });

  // node_modules/wouter-preact/src/react-deps.js
  function is(x5, y5) {
    return x5 === y5 && (x5 !== 0 || 1 / x5 === 1 / y5) || x5 !== x5 && y5 !== y5;
  }
  function useSyncExternalStore(subscribe, getSnapshot, getSSRSnapshot) {
    if (getSSRSnapshot && !canUseDOM) getSnapshot = getSSRSnapshot;
    const value = getSnapshot();
    const [{ _instance }, forceUpdate] = d2({
      _instance: { _value: value, _getSnapshot: getSnapshot }
    });
    F2(() => {
      _instance._value = value;
      _instance._getSnapshot = getSnapshot;
      if (!is(_instance._value, getSnapshot())) {
        forceUpdate({ _instance });
      }
    }, [subscribe, value, getSnapshot]);
    A2(() => {
      if (!is(_instance._value, _instance._getSnapshot())) {
        forceUpdate({ _instance });
      }
      return subscribe(() => {
        if (!is(_instance._value, _instance._getSnapshot())) {
          forceUpdate({ _instance });
        }
      });
    }, [subscribe]);
    return value;
  }
  function forwardRef(component) {
    return (props) => component(props);
  }
  var canUseDOM, useEvent;
  var init_react_deps = __esm({
    "node_modules/wouter-preact/src/react-deps.js"() {
      init_hooks();
      init_preact();
      init_hooks();
      canUseDOM = !!(typeof window !== "undefined" && typeof window.document !== "undefined" && typeof window.document.createElement !== "undefined");
      useEvent = (fn) => {
        const ref = T2([fn, (...args) => ref[0](...args)]).current;
        F2(() => {
          ref[0] = fn;
        });
        return ref[1];
      };
    }
  });

  // node_modules/wouter-preact/src/use-browser-location.js
  var events, listeners, onLocationChange, subscribeToLocationUpdates, useLocationProperty, currentSearch, useSearch, currentPathname, usePathname, navigate, useBrowserLocation, patchKey;
  var init_use_browser_location = __esm({
    "node_modules/wouter-preact/src/use-browser-location.js"() {
      init_react_deps();
      events = ["popstate", "pushState", "replaceState", "hashchange"];
      listeners = [];
      onLocationChange = () => listeners.forEach((callback) => callback());
      subscribeToLocationUpdates = (callback) => {
        if (listeners.push(callback) === 1)
          events.forEach((event) => addEventListener(event, onLocationChange));
        return () => {
          listeners = listeners.filter((listener) => listener !== callback);
          if (!listeners.length)
            events.forEach((event) => removeEventListener(event, onLocationChange));
        };
      };
      useLocationProperty = (fn, ssrFn) => useSyncExternalStore(subscribeToLocationUpdates, fn, ssrFn);
      currentSearch = () => location.search;
      useSearch = ({ ssrSearch } = {}) => useLocationProperty(
        currentSearch,
        // != null checks for both null and undefined, but allows empty string ""
        // This allows proper hydration: server renders with ssrSearch="?foo",
        // client hydrates with just <Router /> and reads from location.search
        ssrSearch != null ? () => ssrSearch : currentSearch
      );
      currentPathname = () => location.pathname;
      usePathname = ({ ssrPath } = {}) => useLocationProperty(
        currentPathname,
        // != null checks for both null and undefined, but allows empty string ""
        // This allows proper hydration: server renders with ssrPath="/foo",
        // client hydrates with just <Router /> and reads from location.pathname
        ssrPath != null ? () => ssrPath : currentPathname
      );
      navigate = (to, { replace = false, state = null } = {}) => history[replace ? "replaceState" : "pushState"](state, "", to);
      useBrowserLocation = (opts) => [usePathname(opts), navigate];
      patchKey = /* @__PURE__ */ Symbol.for("wouter_v3");
      if (typeof history !== "undefined" && typeof window[patchKey] === "undefined") {
        for (const type of ["pushState", "replaceState"]) {
          const original = history[type];
          history[type] = function() {
            const result = original.apply(this, arguments);
            const event = new Event(type);
            event.arguments = arguments;
            dispatchEvent(event);
            return result;
          };
        }
        Object.defineProperty(window, patchKey, { value: true });
      }
    }
  });

  // node_modules/wouter-preact/src/paths.js
  var baseDefaults, absolutePath, relativePath, unescape, sanitizeSearch;
  var init_paths = __esm({
    "node_modules/wouter-preact/src/paths.js"() {
      baseDefaults = (base = "") => base === "/" ? "" : base;
      absolutePath = (to, base) => to[0] === "~" ? to.slice(1) : baseDefaults(base) + to;
      relativePath = (base, path) => {
        base = unescape(baseDefaults(base));
        path = unescape(path);
        return !base || (path.toLowerCase() + "/").startsWith(base.toLowerCase() + "/") ? path.slice(base.length) || "/" : "~" + path;
      };
      unescape = (str) => {
        try {
          return str.includes("%") ? decodeURI(str) : str;
        } catch (_e) {
          return str;
        }
      };
      sanitizeSearch = (search) => unescape(search[0] === "?" ? search.slice(1) : search);
    }
  });

  // node_modules/wouter-preact/src/index.js
  var src_exports = {};
  __export(src_exports, {
    Link: () => Link,
    Redirect: () => Redirect,
    Route: () => Route,
    Router: () => Router,
    Switch: () => Switch,
    matchRoute: () => matchRoute,
    useLocation: () => useLocation,
    useParams: () => useParams,
    useRoute: () => useRoute,
    useRouter: () => useRouter,
    useSearch: () => useSearch2,
    useSearchParams: () => useSearchParams
  });
  function useSearchParams() {
    const [location2, navigate2] = useLocation();
    const search = useSearch2();
    const searchParams = b2(() => new URLSearchParams(search), [search]);
    let tempSearchParams = searchParams;
    const setSearchParams = useEvent((nextInit, options) => {
      tempSearchParams = new URLSearchParams(
        typeof nextInit === "function" ? nextInit(tempSearchParams) : nextInit
      );
      const paramsStr = tempSearchParams.toString();
      navigate2(location2 + (paramsStr ? "?" + paramsStr : ""), options);
    });
    return [searchParams, setSearchParams];
  }
  var defaultRouter, RouterCtx, useRouter, Params0, ParamsCtx, useParams, usePathnameFromRouter, useLocationFromRouter, useLocation, useSearch2, matchRoute, useRoute, Router, h_route, useCachedParams, Route, Link, flattenChildren, Switch, Redirect;
  var init_src = __esm({
    "node_modules/wouter-preact/src/index.js"() {
      init_dist();
      init_use_browser_location();
      init_react_deps();
      init_paths();
      defaultRouter = {
        hook: useBrowserLocation,
        searchHook: useSearch,
        parser: parse,
        base: "",
        // this option is used to override the current location during SSR
        ssrPath: void 0,
        ssrSearch: void 0,
        // optional context to track render state during SSR
        ssrContext: void 0,
        // customizes how `href` props are transformed for <Link />
        hrefs: (x5) => x5,
        // wraps navigate calls, useful for view transitions
        aroundNav: (n3, t4, o4) => n3(t4, o4)
      };
      RouterCtx = K(defaultRouter);
      useRouter = () => w2(RouterCtx);
      Params0 = {};
      ParamsCtx = K(Params0);
      useParams = () => w2(ParamsCtx);
      usePathnameFromRouter = (router, base = router.base) => relativePath(base, router.hook(router)[0]);
      useLocationFromRouter = (router) => {
        const [location2, navigate2] = router.hook(router);
        return [
          relativePath(router.base, location2),
          useEvent(
            (to, opts) => router.aroundNav(navigate2, absolutePath(to, router.base), opts)
          )
        ];
      };
      useLocation = () => useLocationFromRouter(useRouter());
      useSearch2 = () => {
        const router = useRouter();
        return sanitizeSearch(router.searchHook(router));
      };
      matchRoute = (parser, route, path, loose) => {
        const { pattern, keys } = route instanceof RegExp ? { pattern: route } : parser(route || "*", loose);
        const result = pattern.exec(path);
        if (!result) return [false, null];
        const params = {};
        for (let i4 = 1; i4 < result.length; i4++) params[i4 - 1] = result[i4];
        if (keys) {
          for (let i4 = 0; i4 < keys.length; i4++) params[keys[i4]] = result[i4 + 1];
        } else {
          Object.assign(params, result.groups);
        }
        return loose ? [true, params, result[0]] : [true, params];
      };
      useRoute = (pattern) => {
        const router = useRouter();
        const absolute = /^~\//.test(pattern);
        return matchRoute(
          router.parser,
          absolute ? pattern.slice(1) : pattern,
          usePathnameFromRouter(router, absolute ? "" : router.base)
        );
      };
      Router = ({ children, ...props }) => {
        const parent_ = useRouter();
        const parent = props.hook ? defaultRouter : parent_;
        let value = parent;
        if (props.ssrPath) {
          const [path, search = props.ssrSearch ?? ""] = props.ssrPath.split("?");
          if (path) props.ssrSearch = search, props.ssrPath = path;
        }
        props.hrefs = props.hrefs ?? props.hook?.hrefs;
        props.searchHook = props.searchHook ?? props.hook?.searchHook;
        let ref = T2(parent), prev = ref.current, next = prev;
        for (let k5 in parent) {
          const option = k5 === "base" ? (
            /* base is special case, it is appended to the parent's base */
            parent[k5] + (props[k5] ?? "")
          ) : props[k5] ?? parent[k5];
          if (option !== next[k5]) {
            if (prev === next) ref.current = next = { ...next };
            next[k5] = option;
          }
          if (option !== parent[k5] || option !== value[k5]) value = next;
        }
        return g(RouterCtx.Provider, { value }, children);
      };
      h_route = ({ children, component }, params) => {
        if (component) return g(component, { params });
        return typeof children === "function" ? children(params) : children;
      };
      useCachedParams = (value) => {
        let prev = T2(Params0);
        const curr = prev.current, keys = Object.keys(value);
        return prev.current = // Update cache if number of params changed or any value changed
        keys.length !== Object.keys(curr).length || keys.some((k5) => value[k5] !== curr[k5]) ? value : curr;
      };
      Route = ({ path, nest, match, ...renderProps }) => {
        const router = useRouter();
        const location2 = usePathnameFromRouter(router);
        const [matches, routeParams, base] = (
          // `match` is a special prop to give up control to the parent,
          // it is used by the `Switch` to avoid double matching
          match ?? matchRoute(router.parser, path, location2, nest)
        );
        const params = useCachedParams({ ...useParams(), ...routeParams });
        if (!matches) return null;
        const children = h_route(renderProps, params);
        return g(
          ParamsCtx.Provider,
          { value: params },
          base ? g(Router, { base }, children) : children
        );
      };
      Link = forwardRef((props, ref) => {
        const router = useRouter();
        const [currentPath, navigate2] = useLocationFromRouter(router);
        const {
          to = "",
          href: targetPath = to,
          onClick: _onClick,
          asChild,
          children,
          className: cls,
          /* eslint-disable no-unused-vars */
          replace,
          state,
          transition,
          /* eslint-enable no-unused-vars */
          ...linkProps
        } = props;
        linkProps.onClick = useEvent((event) => {
          if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || event.button !== 0)
            return;
          _onClick?.(event);
          if (!event.defaultPrevented) {
            event.preventDefault();
            navigate2(targetPath, props);
          }
        });
        linkProps.href = router.hrefs(
          targetPath[0] === "~" ? targetPath.slice(1) : router.base + targetPath,
          router
          // pass router as a second argument for convinience
        );
        if (cls !== void 0)
          linkProps.className = cls?.call ? cls(currentPath === targetPath) : cls;
        if (ref) linkProps.ref = ref;
        return asChild && i(children) ? k(children, linkProps) : g("a", linkProps, children);
      });
      flattenChildren = (children, result = []) => {
        if (Array.isArray(children)) {
          for (const c4 of children)
            flattenChildren(c4 && c4.type === M ? c4.props.children : c4, result);
        } else result.push(children);
        return result;
      };
      Switch = ({ children, location: location2 }) => {
        const router = useRouter();
        const originalLocation = usePathnameFromRouter(router);
        for (const element of flattenChildren(children)) {
          let match = 0;
          if (i(element) && // we don't require an element to be of type Route,
          // but we do require it to contain a truthy `path` prop.
          // this allows to use different components that wrap Route
          // inside of a switch, for example <AnimatedRoute />.
          (match = matchRoute(
            router.parser,
            element.props.path,
            location2 || originalLocation,
            element.props.nest
          ))[0])
            return k(element, { match });
        }
        return null;
      };
      Redirect = (props) => {
        const { to, href = to } = props;
        const router = useRouter();
        const [, navigate2] = useLocationFromRouter(router);
        const redirect = useEvent(() => navigate2(to || href, props));
        const { ssrContext } = router;
        F2(() => {
          redirect();
        }, []);
        if (ssrContext) {
          ssrContext.redirectTo = to;
        }
        return null;
      };
    }
  });

  // node_modules/wouter-preact/src/memory-location.js
  var memory_location_exports = {};
  __export(memory_location_exports, {
    memoryLocation: () => memoryLocation
  });
  var memoryLocation;
  var init_memory_location = __esm({
    "node_modules/wouter-preact/src/memory-location.js"() {
      init_react_deps();
      memoryLocation = ({
        path = "/",
        searchPath = "",
        state: initialState = null,
        static: staticLocation,
        record
      } = {}) => {
        let initialPath = path;
        if (searchPath) {
          initialPath += path.split("?")[1] ? "&" : "?";
          initialPath += searchPath;
        }
        let [currentPath, currentSearch2 = ""] = initialPath.split("?");
        let currentState = initialState;
        const history2 = [initialPath];
        let listeners2 = [];
        const navigateImplementation = (path2, { replace = false, state } = {}) => {
          if (record)
            history2[replace && history2.length ? history2.length - 1 : history2.length] = path2;
          [currentPath, currentSearch2 = ""] = path2.split("?");
          if (state !== void 0) currentState = state;
          listeners2.forEach((cb) => cb());
        };
        const navigate2 = !staticLocation ? navigateImplementation : () => null;
        const subscribe = (cb) => {
          listeners2 = [...listeners2, cb];
          return () => {
            listeners2 = listeners2.filter((i4) => i4 !== cb);
          };
        };
        const useMemoryLocation = () => [
          useSyncExternalStore(subscribe, () => currentPath),
          navigate2
        ];
        const useMemoryQuery = () => useSyncExternalStore(subscribe, () => currentSearch2);
        useMemoryLocation.searchHook = useMemoryQuery;
        function reset() {
          history2.length = 0;
          navigateImplementation(initialPath, { state: initialState });
        }
        return Object.defineProperty(
          {
            hook: useMemoryLocation,
            searchHook: useMemoryQuery,
            navigate: navigate2,
            history: record ? history2 : void 0,
            reset: record ? reset : void 0
          },
          "state",
          { enumerable: true, get: () => currentState }
        );
      };
    }
  });

  // src/relative-time.js
  var require_relative_time = __commonJS({
    "src/relative-time.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      var mounted = /* @__PURE__ */ new WeakSet();
      var zone;
      var exact;
      var time = execution.module("relative-time", {
        value: function(raw) {
          if (typeof raw === "number") return raw > 0 && Number.isFinite(new Date(raw).getTime()) ? raw : null;
          if (typeof raw !== "string") return null;
          const m5 = /^(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d):(\d\d)(?:\.\d+)?(Z|[+-](\d\d):(\d\d))$/i.exec(raw);
          if (!m5 || +m5[2] < 1 || +m5[2] > 12 || +m5[3] < 1 || +m5[3] > new Date(Date.UTC(+m5[1], +m5[2], 0)).getUTCDate() || +m5[4] > 23 || +m5[5] > 59 || +m5[6] > 59 || +(m5[8] || 0) > 23 || +(m5[9] || 0) > 59) return null;
          const ms = Date.parse(raw);
          return ms > 0 && Number.isFinite(ms) ? ms : null;
        },
        seconds: function(raw) {
          return typeof raw === "number" ? time.value(raw * 1e3) : null;
        },
        label: function(ms, now = Date.now()) {
          const delta = (ms - now) / 1e3, age = Math.abs(delta);
          const [unit, size] = age < 3600 ? ["m", 60] : age < 86400 ? ["h", 3600] : ["d", 86400];
          return (delta > 0 ? "in " : "") + (age < 60 ? "<1m" : Math.floor(age / size) + unit);
        },
        detail: function(ms, context) {
          const current = Intl.DateTimeFormat().resolvedOptions().timeZone;
          if (current !== zone) {
            zone = current;
            exact = new Intl.DateTimeFormat("en-GB", { year: "numeric", month: "short", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23", timeZone: zone, timeZoneName: "longOffset" });
          }
          return context + " \xB7 " + exact.format(ms) + " \xB7 " + zone;
        },
        stamp: function(raw, context = "Time", reason, focus = true) {
          const ms = time.value(raw), full = ms === null ? context + " \xB7 Time unknown: " + (reason || (raw == null ? "Source timestamp unavailable" : "Source timestamp invalid or missing a time zone")) : time.detail(ms, context);
          return h5(ms === null ? "span" : "time", { class: "relative-time" + (ms === null ? " time-unknown" : ms > Date.now() ? " time-future" : ""), ...ms === null ? {} : { dateTime: new Date(ms).toISOString(), "data-time-ms": ms, "data-time-source": raw, "data-time-context": context }, title: full, "aria-label": full, tabIndex: focus ? 0 : void 0 }, ms === null ? "Time unknown" : time.label(ms));
        },
        refresh: function(root, now = Date.now()) {
          for (const el of root.querySelectorAll("time[data-time-ms]")) {
            const next = time.label(Number(el.dataset.timeMs), now);
            if (el.textContent !== next) el.textContent = next;
          }
        },
        mount: function(root) {
          if (mounted.has(root)) return;
          mounted.add(root);
          const document2 = root.ownerDocument, tip = document2.createElement("div");
          tip.className = "time-tooltip";
          tip.setAttribute("role", "tooltip");
          tip.hidden = true;
          document2.body.append(tip);
          let target = null;
          const hide = () => {
            tip.hidden = true;
            target = null;
          }, show = (node) => {
            target = node?.closest(".relative-time,[data-tooltip]") || node?.closest("button,summary")?.querySelector(".relative-time");
            if (!target) return hide();
            if (target.dataset.timeMs) {
              target.title = time.detail(Number(target.dataset.timeMs), target.dataset.timeContext);
              target.setAttribute("aria-label", target.title);
            }
            tip.textContent = target.title;
            tip.hidden = false;
            const r4 = target.getBoundingClientRect();
            tip.style.left = Math.max(4, Math.min(r4.left, document2.documentElement.clientWidth - tip.offsetWidth - 4)) + "px";
            tip.style.top = Math.max(4, Math.min(r4.bottom + 4, document2.documentElement.clientHeight - tip.offsetHeight - 4)) + "px";
          };
          ui.listen(root, "focusin", (e4) => show(e4.target));
          ui.listen(root, "focusout", hide);
          ui.listen(root, "mouseover", (e4) => show(e4.target));
          ui.listen(root, "mouseout", (e4) => {
            if (!tip.contains(e4.relatedTarget)) hide();
          });
          ui.listen(root, "keydown", (e4) => {
            if (e4.key === "Escape") hide();
          });
          ui.listen(root, "scroll", hide, true);
          const tick = () => {
            if (!root.isConnected) {
              tip.remove();
              return;
            }
            time.refresh(root);
            if (target && !target.isConnected) hide();
            ui.defer(tick, 6e4 - Date.now() % 6e4 + 1);
          };
          tick();
        }
      });
      module.exports = time;
    }
  });

  // src/task-health.js
  var require_task_health = __commonJS({
    "src/task-health.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var { execution } = require_ui_runtime();
      var time = require_relative_time();
      module.exports = execution.module("task-health", {
        warning: function(t4) {
          return ["doing", "review"].includes(t4?.state) ? t4?.heartbeat?.warning || (t4?.heartbeat ? null : "Execution heartbeat unknown: source unavailable") : null;
        },
        details: function(t4) {
          const b5 = t4?.heartbeat, p5 = b5?.progress;
          const stamp = (seconds, source) => time.stamp(time.seconds(seconds), source, "No attributable observation");
          return h5("section", { class: "task-health", "aria-label": "Execution health" }, h5("strong", null, "Execution health"), h5(
            "dl",
            null,
            h5("dt", null, "Host / attempt"),
            h5("dd", null, (b5?.host || t4?.host || "unknown") + " / " + (b5?.attempt ?? t4?.attempt ?? "unknown")),
            h5("dt", null, "Heartbeat source"),
            h5("dd", null, b5?.source || "unknown: source unavailable"),
            h5("dt", null, "Last observation / age"),
            h5("dd", null, stamp(b5?.observed, b5?.source || "Host heartbeat")),
            h5("dt", null, "Freshness"),
            h5("dd", null, (b5?.freshness || "unknown") + " \xB7 " + (b5?.reason || "Heartbeat unavailable")),
            h5("dt", null, "Executor"),
            h5("dd", null, (b5?.executor?.state || "unknown") + " \xB7 " + (b5?.executor?.reason || "No current native work confirmed")),
            h5("dt", null, "Execution progress"),
            h5("dd", null, (p5?.status || "unknown") + " \xB7 " + (p5?.reason || "Native progress unavailable")),
            h5("dt", null, "Progress source"),
            h5("dd", null, (p5?.source || "unknown") + (p5?.clock ? " \xB7 " + p5.clock : "")),
            h5("dt", null, "Progress observation"),
            h5("dd", null, stamp(p5?.observed, "Native progress observation")),
            h5("dt", null, "Last completed event"),
            h5("dd", null, stamp(p5?.last_event, "Native completed event")),
            h5("dt", null, "Pending tools"),
            h5("dd", null, p5?.pending_tools ?? "unknown"),
            h5("dt", null, "Tool deadline"),
            h5("dd", null, p5?.deadline ? stamp(p5.deadline, "Declared tool deadline") : p5?.pending_tools ? "No declared deadline; 15m progress window" : "not applicable")
          ));
        }
      });
    }
  });

  // src/ui-icons.json
  var require_ui_icons = __commonJS({
    "src/ui-icons.json"(exports, module) {
      module.exports = { activity: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-activity"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2" />\n</svg>\n', "always-on-top": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-copy taskq-always-on-top"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="14" height="14" x="8" y="8" rx="2" ry="2" fill="currentColor" />\n  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />\n</svg>\n', archive: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-archive"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="20" height="5" x="2" y="3" rx="1" />\n  <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />\n  <path d="M10 12h4" />\n</svg>\n', "archive-restore": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-archive-restore"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="20" height="5" x="2" y="3" rx="1" />\n  <path d="M4 8v11a2 2 0 0 0 2 2h2" />\n  <path d="M20 8v11a2 2 0 0 1-2 2h-2" />\n  <path d="m9 15 3-3 3 3" />\n  <path d="M12 12v9" />\n</svg>\n', "arrow-down": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-arrow-down"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 5v14" />\n  <path d="m19 12-7 7-7-7" />\n</svg>\n', "arrow-left": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-arrow-left"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m12 19-7-7 7-7" />\n  <path d="M19 12H5" />\n</svg>\n', "arrow-up": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-arrow-up"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m5 12 7-7 7 7" />\n  <path d="M12 19V5" />\n</svg>\n', bell: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-bell"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10.268 21a2 2 0 0 0 3.464 0" />\n  <path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326" />\n</svg>\n', bot: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-bot"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 8V4H8" />\n  <rect width="16" height="12" x="4" y="8" rx="2" />\n  <path d="M2 14h2" />\n  <path d="M20 14h2" />\n  <path d="M15 13v2" />\n  <path d="M9 13v2" />\n</svg>\n', "chart-no-axes-column": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-chart-no-axes-column"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M5 21v-6" />\n  <path d="M12 21V3" />\n  <path d="M19 21V9" />\n</svg>\n', check: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-check"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M20 6 9 17l-5-5" />\n</svg>\n', "check-check": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-check-check"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M18 6 7 17l-5-5" />\n  <path d="m22 10-7.5 7.5L13 16" />\n</svg>\n', "circle-alert": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-alert"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <line x1="12" x2="12" y1="8" y2="12" />\n  <line x1="12" x2="12.01" y1="16" y2="16" />\n</svg>\n', "circle-check": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-check"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="m9 12 2 2 4-4" />\n</svg>\n', "circle-question-mark": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-question-mark"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />\n  <path d="M12 17h.01" />\n</svg>\n', "circle-slash": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-slash"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <line x1="9" x2="15" y1="15" y2="9" />\n</svg>\n', copy: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-copy"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />\n  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />\n</svg>\n', download: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-download"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 15V3" />\n  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />\n  <path d="m7 10 5 5 5-5" />\n</svg>\n', ellipsis: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-ellipsis"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="1" />\n  <circle cx="19" cy="12" r="1" />\n  <circle cx="5" cy="12" r="1" />\n</svg>\n', "external-link": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-external-link"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15 3h6v6" />\n  <path d="M10 14 21 3" />\n  <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />\n</svg>\n', "file-plus-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-file-plus-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M11.35 22H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.706.706l3.588 3.588A2.4 2.4 0 0 1 20 8v5.35" />\n  <path d="M14 2v5a1 1 0 0 0 1 1h5" />\n  <path d="M14 19h6" />\n  <path d="M17 16v6" />\n</svg>\n', "file-text": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-file-text"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" />\n  <path d="M14 2v5a1 1 0 0 0 1 1h5" />\n  <path d="M10 9H8" />\n  <path d="M16 13H8" />\n  <path d="M16 17H8" />\n</svg>\n', focus: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-focus"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="3" />\n  <path d="M3 7V5a2 2 0 0 1 2-2h2" />\n  <path d="M17 3h2a2 2 0 0 1 2 2v2" />\n  <path d="M21 17v2a2 2 0 0 1-2 2h-2" />\n  <path d="M7 21H5a2 2 0 0 1-2-2v-2" />\n</svg>\n', "folder-open": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-folder-open"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />\n</svg>\n', "git-merge": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-git-merge"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="18" cy="18" r="3" />\n  <circle cx="6" cy="6" r="3" />\n  <path d="M6 21V9a9 9 0 0 0 9 9" />\n</svg>\n', "git-pull-request": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-git-pull-request"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="18" cy="18" r="3" />\n  <circle cx="6" cy="6" r="3" />\n  <path d="M13 6h3a2 2 0 0 1 2 2v7" />\n  <line x1="6" x2="6" y1="9" y2="21" />\n</svg>\n', grip: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-grip"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="5" r="1" />\n  <circle cx="19" cy="5" r="1" />\n  <circle cx="5" cy="5" r="1" />\n  <circle cx="12" cy="12" r="1" />\n  <circle cx="19" cy="12" r="1" />\n  <circle cx="5" cy="12" r="1" />\n  <circle cx="12" cy="19" r="1" />\n  <circle cx="19" cy="19" r="1" />\n  <circle cx="5" cy="19" r="1" />\n</svg>\n', history: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-history"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />\n  <path d="M3 3v5h5" />\n  <path d="M12 7v5l4 2" />\n</svg>\n', house: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-house"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8" />\n  <path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />\n</svg>\n', inbox: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-inbox"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />\n  <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />\n</svg>\n', info: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-info"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="M12 16v-4" />\n  <path d="M12 8h.01" />\n</svg>\n', "list-filter": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-list-filter"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M2 5h20" />\n  <path d="M6 12h12" />\n  <path d="M9 19h6" />\n</svg>\n', "list-ordered": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-list-ordered"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M11 5h10" />\n  <path d="M11 12h10" />\n  <path d="M11 19h10" />\n  <path d="M4 4h1v5" />\n  <path d="M4 9h2" />\n  <path d="M6.5 20H3.4c0-1 2.6-1.925 2.6-3.5a1.5 1.5 0 0 0-2.6-1.02" />\n</svg>\n', "maximize-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-maximize-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15 3h6v6" />\n  <path d="m21 3-7 7" />\n  <path d="m3 21 7-7" />\n  <path d="M9 21H3v-6" />\n</svg>\n', "messages-square": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-messages-square"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M16 10a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 14.286V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />\n  <path d="M20 9a2 2 0 0 1 2 2v10.286a.71.71 0 0 1-1.212.502l-2.202-2.202A2 2 0 0 0 17.172 19H10a2 2 0 0 1-2-2v-1" />\n</svg>\n', "minimize-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-minimize-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m14 10 7-7" />\n  <path d="M20 10h-6V4" />\n  <path d="m3 21 7-7" />\n  <path d="M4 14h6v6" />\n</svg>\n', "octagon-x": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-octagon-x"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m15 9-6 6" />\n  <path d="M2.586 16.726A2 2 0 0 1 2 15.312V8.688a2 2 0 0 1 .586-1.414l4.688-4.688A2 2 0 0 1 8.688 2h6.624a2 2 0 0 1 1.414.586l4.688 4.688A2 2 0 0 1 22 8.688v6.624a2 2 0 0 1-.586 1.414l-4.688 4.688a2 2 0 0 1-1.414.586H8.688a2 2 0 0 1-1.414-.586z" />\n  <path d="m9 9 6 6" />\n</svg>\n', "panel-left-close": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-panel-left-close"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n  <path d="M9 3v18" />\n  <path d="m16 15-3-3 3-3" />\n</svg>\n', "panels-top-left": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-panels-top-left"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n  <path d="M3 9h18" />\n  <path d="M9 21V9" />\n</svg>\n', paperclip: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-paperclip"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551" />\n</svg>\n', pause: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pause"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect x="14" y="3" width="5" height="18" rx="1" />\n  <rect x="5" y="3" width="5" height="18" rx="1" />\n</svg>\n', pin: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pin"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 17v5" />\n  <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />\n</svg>\n', "pin-off": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pin-off"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 17v5" />\n  <path d="M15 9.34V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H7.89" />\n  <path d="m2 2 20 20" />\n  <path d="M9 9v1.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h11" />\n</svg>\n', plus: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-plus"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M5 12h14" />\n  <path d="M12 5v14" />\n</svg>\n', "rotate-ccw": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-rotate-ccw"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />\n  <path d="M3 3v5h5" />\n</svg>\n', "rotate-cw": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-rotate-cw"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />\n  <path d="M21 3v5h-5" />\n</svg>\n', "rows-3": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-rows-3"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n  <path d="M21 9H3" />\n  <path d="M21 15H3" />\n</svg>\n', save: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-save"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />\n  <path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7" />\n  <path d="M7 3v4a1 1 0 0 0 1 1h7" />\n</svg>\n', search: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-search"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m21 21-4.34-4.34" />\n  <circle cx="11" cy="11" r="8" />\n</svg>\n', settings: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-settings"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915" />\n  <circle cx="12" cy="12" r="3" />\n</svg>\n', shield: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-shield"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />\n</svg>\n', "sliders-horizontal": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-sliders-horizontal"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10 5H3" />\n  <path d="M12 19H3" />\n  <path d="M14 3v4" />\n  <path d="M16 17v4" />\n  <path d="M21 12h-9" />\n  <path d="M21 19h-5" />\n  <path d="M21 5h-7" />\n  <path d="M8 10v4" />\n  <path d="M8 12H3" />\n</svg>\n', square: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-square"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n</svg>\n', "trash-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-trash-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10 11v6" />\n  <path d="M14 11v6" />\n  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />\n  <path d="M3 6h18" />\n  <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />\n</svg>\n', "triangle-alert": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-triangle-alert"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />\n  <path d="M12 9v4" />\n  <path d="M12 17h.01" />\n</svg>\n', type: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-type"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 4v16" />\n  <path d="M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2" />\n  <path d="M9 20h6" />\n</svg>\n', "undo-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-undo-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M9 14 4 9l5-5" />\n  <path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11" />\n</svg>\n', x: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-x"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M18 6 6 18" />\n  <path d="m6 6 12 12" />\n</svg>\n' };
    }
  });

  // src/project-blocks.js
  var require_project_blocks = __commonJS({
    "src/project-blocks.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var { link, counters, summary, aggregate, task, handle } = execution.module("project-blocks", {
        link: function(href, text, label, session = false) {
          const target = url(href, session);
          return target ? '<a href="' + target + '" data-url="' + target + '"' + (label ? ' aria-label="' + esc(label) + '" title="' + esc(label) + '"' : "") + ">" + text + "</a>" : text;
        },
        // #1592: `counts` ([{state,name,n}] in strip order), `partial` and `open` come from the Rust display model.
        counters: function(counts, project = "", label = "All projects", partial = [], selected = null, open = null, heldStart = null) {
          const total = Number.isSafeInteger(open) && open >= 0 ? open : "?";
          const items = (counts || []).map(({ state, name, n: n3 }) => {
            const [mark, kind] = marks[state] || ["circle", ""];
            if (n3 === 0 && selected === null) return "";
            const value = Number.isSafeInteger(n3) && n3 >= 0 ? n3 : "?";
            const title = label + " \xB7 " + name + ": " + (value === "?" ? "unknown" : value);
            const filter = '<button type="button" class="' + kind + '" data-tqp="tasks" data-project="' + esc(project) + '" data-state="' + state + '"' + (selected === null ? "" : ' aria-pressed="' + selected.includes(state) + '"') + ' aria-label="' + esc(title) + '" title="' + esc(title) + '">' + icon(mark) + (n3 === 0 ? "" : '<span class="tqp-count">' + value + "</span>") + "</button>";
            return state === "held" && heldStart ? '<span class="held-control" role="group" aria-label="Held filter and admission">' + filter + '<button type="button" class="good" data-a="held-review" title="' + esc(heldStart.label) + '" aria-label="' + esc(heldStart.label) + '"' + (heldStart.disabled ? " disabled" : "") + ">" + icon("play") + "</button></span>" : filter;
          }).join("");
          return (partial.length ? '<small class="tqp-partial" title="' + esc(partial.join(", ")) + '">Partial: ' + esc(partial.join(", ")) + "</small>" : "") + (items ? '<div class="tqp-counts" aria-label="' + esc(label) + ' counters">' + items + (selected === null ? "" : '<button type="button" data-a="all-tasks" aria-pressed="' + !selected.length + '" aria-label="All tasks: ' + (total === "?" ? "unknown" : total) + ' open" title="All projects \xB7 open tasks">All tasks ' + total + "</button>") + "</div>" : "");
        },
        summary: function(p5, selected = null, open = null) {
          return '<section class="tqp tqp-project" aria-label="' + esc(p5.name) + '"><div class="tqp-identity"><button type="button" data-tqp="project" data-project="' + esc(p5.id) + '">' + icon(p5.pinned ? "pin" : "folder") + " " + esc(p5.name) + "</button>" + link(p5.board.url, icon(p5.board.kind) + " Board", "Open " + p5.name + " Board") + "</div>" + counters(p5.counts, p5.id, p5.name, p5.partial || [], selected, open) + "</section>";
        },
        aggregate: function(all, selected = null) {
          return '<section class="tqp" aria-label="All projects">' + counters(all.counts, "", "All projects", all.partial || [], selected, all.open) + "</section>";
        },
        task: function(p5, row, global2 = false, compact = false) {
          const name = row.state_name ?? row.state, [mark, kind] = marks[row.state] || ["circle", ""];
          const board = row.key ? row.key.slice(0, row.key.lastIndexOf("/")) : p5.board.url;
          const rt = row.runtime, brand = rt === "claude" ? "claude" : ["codex", "codex-app"].includes(rt) ? "openai" : rt === "antigravity" ? "terminal" : "";
          const remote = row.host && !row.local_session;
          const badges = (remote ? '<span title="Remote host" aria-label="Remote host">' + icon("network") + "</span> " : "") + (rt ? icon(brand) + " " + esc(rt) : "");
          const target = url(row.session_url, !!row.local_session);
          const session = target ? link(row.session_url, icon("terminal"), "Open " + (rt || "agent") + " session", true) : row.host ? "<small" + (row.session_reason ? ' title="' + esc(row.session_reason) + '"' : "") + ">" + (remote ? "Session unavailable" : "Session starting or unavailable") + "</small>" : "";
          const since = !("since" in row) ? "" : row.age == null ? '<small class="tqp-age" title="Age unknown: no logged state change">age ?</small>' : '<small class="tqp-age" title="Since the last state change">' + esc(row.age) + "</small>";
          const metadata = [since, global2 ? esc(p5.name) : "", row.host ? esc(row.host) : "", badges].filter(Boolean).join(" \xB7 ");
          const wait = row.wait ? '<small class="tqp-execution-wait" role="status">' + esc(row.wait) + "</small>" : "";
          const warning = require_task_health().warning(row);
          const health = ["doing", "review"].includes(row.state) ? '<span class="task-health-warning"' + (warning ? ' role="status" tabindex="0" title="' + esc(warning) + '" aria-label="' + esc(warning) + '"' : ' aria-hidden="true"') + ">" + (warning ? icon("triangle-alert") + '<span class="task-health-tip" role="tooltip">' + esc(warning) + "</span>" : "") + "</span>" : "";
          const parked = (row.stage ? '<small class="tqp-stage" title="First open stage of the Acceptance items">Stage ' + esc(row.stage.k) + "/" + esc(row.stage.n) + "</small>" : "") + (row.until ? '<small class="tqp-parked">' + esc(parkedText(row)) + "</small>" : "");
          if (compact) {
            const elapsed = row.age != null ? '<span class="tqp-age">' + esc(row.age) + "</span>" : '<span class="tqp-age" title="Current stage elapsed unknown">?</span>';
            const label = "View " + (rt || "task") + " agent #" + row.n;
            const runtime = brand ? '<button type="button" class="tqp-mark" data-a="workspace" data-page="answer" data-root="' + esc(row.root || p5.root || "") + '" data-board="' + esc(board) + '" data-task="' + esc(row.n) + '" data-revision="' + esc(row.revision) + '" aria-label="' + esc(label) + '" title="' + esc(label) + '">' + icon(brand) + "</button>" : "";
            const provider = p5.board.kind === "gitlab" ? "GitLab" : p5.board.kind === "github" ? "GitHub" : "recorded board";
            const forge = url(row.url) ? '<a class="task-board" href="' + url(row.url) + '" data-url="' + url(row.url) + '" aria-label="Open #' + esc(row.n) + " on " + provider + '" title="Open #' + esc(row.n) + " on " + provider + '">' + icon(p5.board.kind) + "</a>" : '<span class="task-board" aria-disabled="true" title="Issue target unavailable">' + icon(p5.board.kind) + "</span>";
            const run = (row.host && row.local_session === false ? '<span class="tqp-mark" title="Remote host ' + esc(row.host) + '" aria-label="Remote host">' + icon("network") + "</span>" : "") + runtime;
            return '<div class="tqp tqp-task"><div class="tqp-task-head"><span class="task-stage ' + kind + '" aria-label="' + esc(name) + ' \xB7 current stage" title="' + esc(name) + ' \xB7 current stage">' + (compact && row.state === "ask" ? '<button type="button" class="question-shortcut" data-a="workspace" data-page="answer" data-root="' + esc(row.root || p5.root || "") + '" data-board="' + esc(board) + '" data-revision="' + esc(row.revision) + '" data-task="' + esc(row.n) + '" aria-label="Open question #' + esc(row.n) + '" title="Open question #' + esc(row.n) + '">' + icon(mark) + "</button>" : icon(mark)) + "</span>" + forge + '<button type="button" class="task-identity" data-a="task-detail" data-key="' + esc(row.key) + '" aria-controls="taskq-question-details" aria-expanded="false" aria-label="Details for ' + esc(p5.name) + " #" + row.n + '">' + (global2 ? esc(p5.name) + " " : "") + "#" + esc(row.n) + "</button>" + (row.state === "review" && row.enqueued ? '<span class="task-publication">' + esc(name) + "</span>" : "") + '<span class="task-when">' + health + run + elapsed + '</span></div><div class="tqp-task-title">' + link(row.url, esc(row.title), "Open task #" + row.n) + "</div>" + parked + "</div>";
          }
          return '<div class="tqp tqp-task"><div class="tqp-task-head"><span class="' + kind + '" title="' + esc(name) + '" aria-label="' + esc(name) + '">' + icon(mark) + "</span> " + link(row.url, icon(p5.board.kind) + " #" + esc(row.n), "Open task #" + row.n) + ' <span class="tqp-meta">' + metadata + "</span> " + session + " " + health + wait + '</div><div class="tqp-task-title">' + esc(row.title) + "</div>" + parked + "</div>";
        },
        handle: function(event, adapter) {
          const el = event.target.closest("[data-tqp],.tqp a[data-url]");
          if (!el) return false;
          event.preventDefault();
          if (el.dataset.url) adapter.open(el.getAttribute("href"));
          else if (el.dataset.tqp === "project") adapter.project(el.dataset.project);
          else adapter.tasks(el.dataset.project, el.dataset.state);
          return true;
        }
      });
      var assets = { "circle-x": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-x"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="m15 9-6 6" />\n  <path d="m9 9 6 6" />\n</svg>\n', "clock": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-clock"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="M12 6v6l4 2" />\n</svg>\n', "folder": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-folder"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />\n</svg>\n', "circle-dashed": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-dashed"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10.1 2.182a10 10 0 0 1 3.8 0" />\n  <path d="M13.9 21.818a10 10 0 0 1-3.8 0" />\n  <path d="M17.609 3.721a10 10 0 0 1 2.69 2.7" />\n  <path d="M2.182 13.9a10 10 0 0 1 0-3.8" />\n  <path d="M20.279 17.609a10 10 0 0 1-2.7 2.69" />\n  <path d="M21.818 10.1a10 10 0 0 1 0 3.8" />\n  <path d="M3.721 6.391a10 10 0 0 1 2.7-2.69" />\n  <path d="M6.391 20.279a10 10 0 0 1-2.69-2.7" />\n</svg>\n', "circle-pause": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-pause"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <line x1="10" x2="10" y1="15" y2="9" />\n  <line x1="14" x2="14" y1="15" y2="9" />\n</svg>\n', "gitlab": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>GitLab</title><path d="m23.6004 9.5927-.0337-.0862L20.3.9814a.851.851 0 0 0-.3362-.405.8748.8748 0 0 0-.9997.0539.8748.8748 0 0 0-.29.4399l-2.2055 6.748H7.5375l-2.2057-6.748a.8573.8573 0 0 0-.29-.4412.8748.8748 0 0 0-.9997-.0537.8585.8585 0 0 0-.3362.4049L.4332 9.5015l-.0325.0862a6.0657 6.0657 0 0 0 2.0119 7.0105l.0113.0087.03.0213 4.976 3.7264 2.462 1.8633 1.4995 1.1321a1.0085 1.0085 0 0 0 1.2197 0l1.4995-1.1321 2.4619-1.8633 5.006-3.7489.0125-.01a6.0682 6.0682 0 0 0 2.0094-7.003z"/></svg>', "git-pull-request": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-git-pull-request"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="18" cy="18" r="3" />\n  <circle cx="6" cy="6" r="3" />\n  <path d="M13 6h3a2 2 0 0 1 2 2v7" />\n  <line x1="6" x2="6" y1="9" y2="21" />\n</svg>\n', "openai": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>OpenAI</title><path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z"/></svg>', "play": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-play"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" />\n</svg>\n', "terminal": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-terminal"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 19h8" />\n  <path d="m4 17 6-6-6-6" />\n</svg>\n', "message-circle-question": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-message-circle-question"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719" />\n  <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />\n  <path d="M12 17h.01" />\n</svg>\n', "github": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>GitHub</title><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>', "circle": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n</svg>\n', "pin": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pin"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 17v5" />\n  <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />\n</svg>\n', "link": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-link"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />\n  <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />\n</svg>\n', "network": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-network"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect x="16" y="16" width="6" height="6" rx="1" />\n  <rect x="2" y="16" width="6" height="6" rx="1" />\n  <rect x="9" y="2" width="6" height="6" rx="1" />\n  <path d="M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3" />\n  <path d="M12 12V8" />\n</svg>\n', "claude": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>Claude</title><path d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z"/></svg>' };
      assets["triangle-alert"] = require_ui_icons()["triangle-alert"];
      var marks = {
        doing: ["play", "good"],
        review: ["git-pull-request", "review"],
        ask: ["message-circle-question", "ask"],
        failed: ["circle-x", "danger"],
        ready: ["circle", ""],
        waiting: ["link", ""],
        later: ["circle-pause", ""],
        held: ["circle-dashed", ""]
      };
      var { esc } = execution.module("project-blocks", { esc: (x5) => String(x5 ?? "").replace(/[&<>"']/g, (c4) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c4]) });
      var { icon } = execution.module("project-blocks", { icon: (name) => (assets[name] || "").replace("<svg", '<svg aria-hidden="true" focusable="false" data-icon="' + name + '"') });
      var { url } = execution.module("project-blocks", { url: (s4, session = false) => /^(https?:\/\/|codex:\/\/threads\/)/i.test(s4 || "") && (session || /^https?:\/\//i.test(s4)) ? esc(s4) : "" });
      var { parkedText } = execution.module("project-blocks", { parkedText: (row) => row.until ? "Parked until " + new Date(row.until).toLocaleString() + (row.later ? ": " + row.later.replace(/^Reason: /, "") : "") : "" });
      var { destinations } = execution.module("project-blocks", {
        destinations: function(t4) {
          const native = Object.hasOwn(t4, "native_session_url") ? t4.native_session_url : t4.local_session || t4.runtime === "claude" ? t4.session_url : null;
          const progress = Object.hasOwn(t4, "progress_url") ? t4.progress_url : !t4.local_session && ["codex", "codex-app"].includes(t4.runtime) ? t4.session_url : null;
          return [["native", native, t4.native_session_reason || t4.session_reason, "Open native session"], ["progress", progress, t4.progress_reason || t4.session_reason, "View read-only progress"]].map(([kind, href, reason, label]) => ({ kind, label, url: url(href, kind === "native" && t4.local_session) ? href : null, reason: reason || (kind === "native" ? "No verified native session target" : "No read-only progress target published") }));
        }
      });
      var api = { marks, assets, counters, summary, aggregate, task, handle, parked: parkedText, destinations };
      module.exports = api;
    }
  });

  // src/ui-icons.js
  var require_ui_icons2 = __commonJS({
    "src/ui-icons.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var { execution } = require_ui_runtime();
      var assets = { ...require_project_blocks().assets, ...require_ui_icons() };
      module.exports = execution.module("uiIcons", {
        icon: function(name) {
          return h5("span", { class: "ui-icon", "aria-hidden": "true", dangerouslySetInnerHTML: { __html: module.exports.asset(name) } });
        },
        // R43: never a circle that reads as Ready; scripts/check-web.cjs fails the build on an unknown literal name, a data-driven one renders empty.
        asset: function(name) {
          return assets[name] || "";
        },
        names: function() {
          return Object.keys(assets);
        },
        button: function(name, label, props = {}) {
          return h5("button", { type: "button", title: label, "aria-label": label, ...props }, module.exports.icon(name));
        }
      });
    }
  });

  // src/app-shell.js
  var require_app_shell = __commonJS({
    "src/app-shell.js"(exports, module) {
      "use strict";
      var { h: h5, render, options } = (init_preact(), __toCommonJS(preact_exports));
      var { signal } = (init_signals_module(), __toCommonJS(signals_module_exports));
      var { useLayoutEffect, useRef } = (init_hooks(), __toCommonJS(hooks_exports));
      var { Router: Router2, Route: Route2, useLocation: useLocation2 } = (init_src(), __toCommonJS(src_exports));
      var { memoryLocation: memoryLocation2 } = (init_memory_location(), __toCommonJS(memory_location_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      options.debounceRendering = (callback) => ui.defer(callback);
      var api = execution.module("shell", {
        // R07/P-W3: one action row per window (Home, actions, window controls) whose icons never move; Back, breadcrumbs and
        // a title that does not fit take the row below.
        bar: function({ title, back, home, crumbs, actions, windows }) {
          const icons = require_ui_icons2();
          return h5("header", { class: "head" + (back || crumbs ? " title-below" : "") }, home && h5("button", { class: "nav-home", "data-a": "home", title: home, "aria-label": home }, icons.icon("house")), h5("div", { class: "nav-title" }, back && h5("button", { class: "nav-back", "data-a": "back", title: "Back", "aria-label": "Back" }, icons.icon("arrow-left")), crumbs && h5("nav", { class: "breadcrumbs", "aria-label": "Breadcrumb" }, crumbs), h5("h1", { tabindex: "-1", title }, title)), h5("div", { class: "nav-actions" }, actions), h5("div", { class: "window-controls", "aria-label": "Window controls" }, windows));
        },
        // A short title shares the action row; one that would be cut moves below, whole words only.
        fitTitle: function(container) {
          const head = container.querySelector(".head"), title = head?.querySelector("h1");
          if (!title || head.classList.contains("title-below")) return;
          const key = head.clientWidth + "|" + getComputedStyle(head).fontSize + "|" + title.textContent;
          if (head.fitted === key) return;
          head.fitted = key;
          head.removeAttribute("data-title-below");
          if (title.scrollWidth > title.clientWidth + 1) head.setAttribute("data-title-below", "");
        },
        Frame: function({ frame }) {
          const [route] = useLocation2(), element = useRef();
          useLayoutEffect(() => {
            api.fitTitle(element.current);
          });
          useLayoutEffect(() => {
            const el = element.current;
            let width;
            const observer = new ResizeObserver(() => {
              const next = el.clientWidth;
              if (next !== width) {
                width = next;
                api.fitTitle(el);
              }
            });
            observer.observe(el);
            ui.effect("shell.fonts", () => document.fonts.ready, () => {
              if (el.isConnected) api.fitTitle(el);
            });
            return () => observer.disconnect();
          }, []);
          return h5(Route2, { path: frame.route }, h5("div", { ref: element, "class": "shell", "data-shell-route": route, "data-shell-depth": frame.depth }, frame.nodes));
        },
        create: function(container) {
          const location2 = memoryLocation2({ record: true }), frame = signal(null), entries = [];
          const instance = execution.module("shell.instance", {
            entries: function() {
              return entries;
            },
            push: function(state) {
              entries.push(state);
              if (entries.length > 32) {
                entries.shift();
                if (location2.history.length > 1) location2.history.shift();
              }
              location2.navigate(location2.history.at(-1));
            },
            pop: function() {
              const state = entries.pop();
              if (location2.history.length > 1) {
                location2.history.pop();
                location2.navigate(location2.history.at(-1), { replace: true });
              }
              return state;
            },
            clear: function() {
              entries.length = 0;
              location2.history.splice(0, location2.history.length - 1);
            },
            // Route components render under the Wouter Router; shared HTML enters only as islands (dangerouslySetInnerHTML).
            render: function(nodes, route) {
              frame.value = { route, depth: location2.history.length, nodes };
              if (location2.history.at(-1) !== route) location2.navigate(route, { replace: true });
              render(h5(Router2, { hook: location2.hook }, h5(api.Frame, { frame: frame.value })), container);
              api.fitTitle(container);
            }
          });
          return instance;
        }
      });
      module.exports = api;
    }
  });

  // src/sync-view.js
  var require_sync_view = __commonJS({
    "src/sync-view.js"(exports, module) {
      "use strict";
      var { execution } = require_ui_runtime();
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var PHASES = { sending: "Sending", failed: "Failed", mismatch: "Not applied" };
      var api = execution.module("sync-view", {
        clock: function(ms) {
          const d5 = new Date(ms);
          return String(d5.getHours()).padStart(2, "0") + ":" + String(d5.getMinutes()).padStart(2, "0");
        },
        // One line for a row's operation: phase, then its reason.
        line: function(o4) {
          return o4 ? (PHASES[o4.phase] || o4.phase) + (o4.reason ? ": " + o4.reason : "") : "";
        },
        // 'Data from HH:MM' while a board read is older than two polls or missing; null while every read is fresh.
        // The read ages by status.json `age` plus its `tasks_age_seconds`; an idle service waits 3 polls, at most 15 s while a
        // window is open (taskq.md § 4 step 8).
        stale: function(S3) {
          let oldest = null, missing = false;
          for (const p5 of S3?.projects || []) {
            const age = p5.age + p5.tasks_age_seconds, interval = Math.max(p5.poll, Math.min(3 * p5.poll, 15));
            if (p5.alive !== true || !Number.isFinite(age)) {
              missing = true;
              continue;
            }
            if (Number.isFinite(interval) && age > 2 * interval) {
              const at = (S3.now - age) * 1e3;
              oldest = oldest === null ? at : Math.min(oldest, at);
            }
          }
          return oldest !== null ? "Data from " + api.clock(oldest) : missing ? "Data from: unknown" : null;
        },
        // A row's operation as shown in every window; only a failed one offers Retry, which resends the same operation.
        row: function(o4) {
          return o4 ? h5("p", { class: "meta sync " + o4.phase, role: "status" }, api.line(o4), o4.phase === "failed" && h5("button", { class: "compact-action", "data-a": "sync-retry", "data-root": o4.root, "data-id": o4.id }, "Retry")) : null;
        },
        // The window's one stale-data line, or nothing while every read is fresh.
        staleRow: function(S3) {
          const t4 = api.stale(S3);
          return t4 ? h5("p", { class: "meta stale", role: "status" }, t4) : null;
        }
      });
      module.exports = { phases: PHASES, line: api.line, stale: api.stale, row: api.row, staleRow: api.staleRow };
    }
  });

  // src/task-views.js
  var require_task_views = __commonJS({
    "src/task-views.js"(exports, module) {
      "use strict";
      var { execution } = require_ui_runtime();
      var { marks } = require_project_blocks();
      var states = Object.keys(marks);
      var builtins = [["all", "All tasks"], ["completed", "Completed"]];
      var periods = ["7d", "30d", "all"];
      var outcomes = ["success", "not_planned", "unknown"];
      var api = execution.module("taskViews", {
        text: function(v4, n3) {
          return typeof v4 === "string" ? v4.slice(0, n3) : "";
        },
        identity: function(p5) {
          try {
            const u4 = new URL(p5.issues);
            return (p5.board_kind || "unknown") + "://" + u4.host.toLowerCase() + u4.pathname.replace(/\/(?:-\/)?issues\/?$/, "").replace(/\/$/, "");
          } catch {
            return null;
          }
        },
        criteria: function(v4 = {}) {
          v4 = v4 && typeof v4 === "object" && !Array.isArray(v4) ? v4 : {};
          return { states: states.filter((s4) => Array.isArray(v4.states) && v4.states.includes(s4)), projects: [...new Set(Array.isArray(v4.projects) ? v4.projects.filter((p5) => typeof p5 === "string" && p5.length <= 2048) : [])], query: api.text(v4.query, 1024), group: v4.group === "combined" ? "combined" : "project", period: periods.includes(v4.period) ? v4.period : "30d", outcomes: outcomes.filter((o4) => Array.isArray(v4.outcomes) && v4.outcomes.includes(o4)) };
        },
        view: function(v4) {
          return { id: v4.id, name: v4.name, pinned: v4.pinned !== false, saved: api.criteria(v4.saved), working: api.criteria(v4.working || v4.saved), scroll: Number.isFinite(v4.scroll) && v4.scroll >= 0 ? v4.scroll : 0, open: api.text(v4.open, 4096) || null, draft: api.text(v4.draft, 80) };
        },
        pin: function(p5) {
          return p5 && typeof p5.ref === "string" && p5.ref.length <= 4096 ? { ref: p5.ref, title: api.text(p5.title, 1024), url: api.text(p5.url, 4096), ...Number.isFinite(p5.closedSeen) && p5.closedSeen > 0 ? { closedSeen: p5.closedSeen } : {} } : null;
        },
        builtin: function(id) {
          return builtins.some(([b5]) => b5 === id);
        },
        normalize: function(value, legacy = {}) {
          const valid = (v4) => v4 && typeof v4.id === "string" && /^[a-z0-9-]{1,64}$/.test(v4.id) && typeof v4.name === "string" && v4.name.trim() && v4.name.length <= 80;
          const found = Array.isArray(value?.views) ? value.views.filter(valid).slice(0, 64) : [];
          const views = [...new Map([...builtins.map(([id, name]) => found.find((v4) => v4.id === id) || { id, name, pinned: true, working: id === "all" ? { states: legacy.taskFilter || [] } : {}, scroll: id === "all" ? legacy.scroll || 0 : 0 }), ...found].map((v4) => [v4.id, api.view({ ...v4, name: builtins.find(([b5]) => b5 === v4.id)?.[1] || v4.name })])).values()];
          const pins = [...new Map([...Array.isArray(value?.pins) ? value.pins : [], ...found.flatMap((v4) => Array.isArray(v4.pins) ? v4.pins : [])].map(api.pin).filter(Boolean).map((p5) => [p5.ref, p5])).values()].slice(0, 256);
          const u4 = value?.undo, undo = u4 && valid(u4.view) && !api.builtin(u4.view.id) ? { view: api.view(u4.view), index: Number.isInteger(u4.index) ? u4.index : views.length, selected: u4.selected === true, fallback: typeof u4.fallback === "string" ? u4.fallback : "" } : null;
          return { schema: 1, collapsed: [...new Set(Array.isArray(value?.collapsed) ? value.collapsed.filter((p5) => typeof p5 === "string" && p5.length <= 2048) : [])], selected: views.some((v4) => v4.id === value?.selected) ? value.selected : "all", views, pins, undo };
        },
        current: function(model) {
          return model.views.find((v4) => v4.id === model.selected) || model.views[0];
        },
        dirty: function(view) {
          return JSON.stringify(view.saved) !== JSON.stringify(view.working);
        },
        scope: function(projects, criteria) {
          return projects.filter((p5) => !criteria.projects.length || criteria.projects.includes(api.identity(p5)));
        },
        matches: function(t4, query) {
          return !query || [t4.title, t4.n, t4.project, t4.runtime, t4.host, t4.result].filter((v4) => v4 != null).join(" ").toLocaleLowerCase().includes(query.toLocaleLowerCase());
        },
        // #1592: `rank` is the display model's global order (state, priority, number, board); rows without one go last.
        compare: function(a4, b5) {
          return (a4.rank ?? Infinity) - (b5.rank ?? Infinity) || 0;
        },
        save: function(model) {
          const v4 = api.current(model);
          v4.saved = api.criteria(v4.working);
        },
        reset: function(model) {
          const v4 = api.current(model);
          v4.working = api.criteria(v4.saved);
        },
        add: function(model, name, id) {
          name = name.trim();
          if (!name || name.length > 80 || model.views.length >= 64) return false;
          const from = api.current(model), criteria = api.criteria(from.working);
          from.draft = "";
          model.views.push(api.view({ id, name, saved: criteria }));
          model.selected = id;
          return true;
        },
        // P-V1: built-ins stay; one latest undo record; deleting the active view falls back to All tasks with its own context.
        remove: function(model, id) {
          const index = model.views.findIndex((v4) => v4.id === id);
          if (index < 0 || api.builtin(id)) return false;
          const selected = model.selected === id;
          if (selected) model.selected = "all";
          model.undo = { view: model.views.splice(index, 1)[0], index, selected, fallback: JSON.stringify(api.current(model).working) };
          return true;
        },
        undo: function(model) {
          const u4 = model.undo;
          if (!u4) return false;
          model.undo = null;
          if (model.views.some((v4) => v4.id === u4.view.id) || model.views.length >= 64) return false;
          model.views.splice(Math.min(u4.index, model.views.length), 0, u4.view);
          if (u4.selected && model.selected === "all" && JSON.stringify(api.current(model).working) === u4.fallback) model.selected = u4.view.id;
          return true;
        },
        pinned: function(model, ref) {
          return model.pins.some((p5) => p5.ref === ref);
        },
        toggle: function(model, pin) {
          const i4 = model.pins.findIndex((p5) => p5.ref === pin.ref);
          if (i4 >= 0) model.pins.splice(i4, 1);
          else model.pins.push(api.pin(pin));
        }
      });
      module.exports = api;
    }
  });

  // src/resource-controls.js
  var require_resource_controls = __commonJS({
    "src/resource-controls.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var { scopes, freshness, policy, gate, age, usage, applied, headline, fields, edited, problem, changes, review, fresh, status, step, render, input, mount, workFields, freshWork, workStatus, workStep, workRender, mountWork, replayWork, replay } = execution.module("resource-controls", {
        scopes: function(group) {
          const t4 = group.targets || [];
          const host = t4.find((x5) => x5.target.project_id == null && x5.target.runtime_id == null), project = t4.find((x5) => x5.target.project_id != null && x5.target.runtime_id == null);
          return { host, project, observation: schemaOf(host, "taskq.host-capacity/1") ? host : project, runtimes: t4.filter((x5) => x5.target.runtime_id != null) };
        },
        freshness: function(group, now) {
          const p5 = scopes(group).observation, obs = group.observed_at, s4 = p5 && p5.sample;
          const status2 = obs == null || now < obs ? null : now - obs;
          const sample = s4 && s4.sampled_at != null && s4.age_seconds != null && status2 != null && s4.sampled_at <= obs ? s4.age_seconds + status2 : null;
          return { status: status2, sample, fresh: status2 != null && status2 <= FRESH };
        },
        policy: function(group) {
          const h5 = scopes(group).host, c4 = h5 && h5.current_policy || {};
          if (schemaOf(h5, "taskq.host-capacity/1")) return "Host limits \xB7 " + (c4.maximum == null ? "no maximum" : "maximum " + c4.maximum);
          if (!schemaOf(h5, "taskq.admission/1")) return "Legacy guard \xB7 Fixed limits";
          if (c4.mode === "auto") return "Auto \xB7 " + (c4.maximum == null ? "no host maximum" : "host maximum " + c4.maximum);
          if (c4.mode === "fixed") return "Fixed";
          return "Policy unknown";
        },
        gate: function(group, now) {
          const p5 = scopes(group).observation, f4 = freshness(group, now);
          if (!p5) return "Unknown: no capacity status";
          if (!f4.fresh) return "Unknown: status " + (f4.status == null ? "missing" : age(f4.status) + " old");
          const g5 = p5.effective_policy && p5.effective_policy.load_gate, bs = p5.blockers || [];
          if (bs.some((b5) => b5.code === "config")) return "Unknown: invalid config";
          if (g5 && g5.enabled === false) return "Off";
          if (!g5) return "Unknown: gate not reported";
          if (f4.sample == null || f4.sample > FRESH) return "Unknown: " + (f4.sample == null ? "no sample yet" : "sample " + age(f4.sample) + " old");
          if (bs.some((b5) => b5.code === "sample" || b5.knowledge === "unknown")) return "Unknown: " + bs.filter((b5) => b5.code === "sample" || b5.knowledge === "unknown").map((b5) => BLOCKERS[b5.code] || BLOCKERS.unknown).join(", ");
          const held = bs.filter((b5) => GATE.includes(b5.code));
          return held.length ? "Waiting: " + [...new Set(held.map((b5) => BLOCKERS[b5.code]))].join(", ") : "Ready";
        },
        age: function(s4) {
          return s4 < 120 ? s4 + " s" : s4 < 7200 ? Math.round(s4 / 60) + " min" : s4 < 172800 ? Math.round(s4 / 3600) + " h" : Math.round(s4 / 86400) + " d";
        },
        usage: function(group, now) {
          const p5 = scopes(group).observation, fresh2 = freshness(group, now).fresh;
          return (p5 && p5.runtime_usage || []).map((u4) => {
            const used = fresh2 && u4.used != null ? u4.used : null, l5 = u4.limit;
            const limit = u4.limit_kind === "unbounded" ? "no cap" : u4.limit_kind === "unknown" || l5 == null && u4.limit_kind !== "disabled" ? "?" : l5 == null ? "none" : String(l5);
            const over = used != null && l5 != null && u4.limit_kind !== "unbounded" && used > l5;
            return {
              runtime: u4.runtime_id,
              used,
              limit,
              over,
              off: u4.limit_kind === "disabled",
              text: (used == null ? "?" : used) + "/" + limit + (over ? " \xB7 over limit" : u4.limit_kind === "disabled" ? " \xB7 off" : "")
            };
          });
        },
        applied: function(group, now) {
          const t4 = group.targets || [];
          if (!t4.length || !freshness(group, now).fresh || t4.some((x5) => x5.effective_revision == null)) return "unknown";
          return t4.every((x5) => x5.current_revision === x5.effective_revision) ? "applied" : "saved";
        },
        headline: function(group, now) {
          return policy(group) + " \xB7 Load gate: " + gate(group, now);
        },
        fields: function(group) {
          const s4 = scopes(group), out = [], hc = s4.host && s4.host.current_policy || {};
          if (editable(s4.host, "mode")) out.push({ key: "host.mode", target: s4.host, field: "mode", label: "Mode", value: hc.mode, choice: ["auto", "fixed"] });
          if (editable(s4.host, "maximum")) out.push({ key: "host.maximum", target: s4.host, field: "maximum", label: "Host maximum", nullable: true, value: hc.maximum, none: "no maximum" });
          for (const r4 of s4.runtimes) if (editable(r4, "runtime_limit")) out.push({
            key: "rt:" + r4.target.runtime_id,
            target: r4,
            field: "runtime_limit",
            label: rtName(r4.target.runtime_id),
            nullable: ["taskq.admission/1", "taskq.host-capacity/1", "taskq.project-limits/1"].includes(schemaFor(r4, "runtime_limit")),
            none: { "taskq.host-capacity/1": "no cap", "taskq.project-limits/1": "no override" }[schemaFor(r4, "runtime_limit")] || null,
            value: r4.current_policy && r4.current_policy.runtime_limit
          });
          const target = editable(s4.host, "load_gate") ? s4.host : s4.project;
          if (editable(target, "load_gate")) {
            const g5 = target.current_policy && target.current_policy.load_gate || {};
            out.push({
              key: "gate.enabled",
              target,
              field: "load_gate",
              label: "Load gate",
              value: g5.enabled,
              bool: true,
              locked: scopes(group).host && scopes(group).host.current_policy && scopes(group).host.current_policy.mode === "auto"
            });
            out.push({ key: "gate.cpu_max", target, field: "load_gate", label: "CPU ceiling %", value: g5.cpu_max, lo: 10, hi: 100 });
            out.push({ key: "gate.memory_min", target, field: "load_gate", label: "Memory floor %", value: g5.memory_min, lo: 1, hi: 95 });
          }
          return out;
        },
        edited: function(group, d5) {
          return fields(group).filter((f4) => d5.touched[f4.key] && d5.values[f4.key] !== shown(f4));
        },
        problem: function(group, d5) {
          const n3 = (v4, lo, hi) => /^\d+$/.test(v4) && Number(v4) >= lo && Number(v4) <= hi;
          for (const f4 of edited(group, d5)) {
            const v4 = d5.values[f4.key];
            if (f4.bool) {
              if (f4.locked && !v4 && d5.values["host.mode"] !== "fixed") return "Auto keeps the load gate on";
              continue;
            }
            if (f4.choice) {
              if (!f4.choice.includes(v4)) return f4.label + ": Auto or Fixed";
              continue;
            }
            if (f4.field === "maximum" && v4 !== "" && !n3(v4, 0, 4294967295)) return f4.label + ": an integer 0 or more, or empty for no maximum";
            if (f4.field === "runtime_limit" && v4 === "") {
              if (!f4.nullable) return f4.label + ": Fixed limits need a number; 0 admits none";
              continue;
            }
            if (f4.field === "runtime_limit" && !n3(v4, 0, 4294967295)) return f4.label + ": an integer 0 or more";
            if (f4.lo != null && !n3(v4, f4.lo, f4.hi)) return f4.label + ": an integer " + f4.lo + ".." + f4.hi;
          }
          if (!edited(group, d5).length) return "No change";
          const now = (k5) => {
            const f4 = fields(group).find((x5) => x5.key === k5);
            return f4 && (d5.touched[k5] ? d5.values[k5] : shown(f4));
          };
          return now("host.mode") === "auto" && now("gate.enabled") === false ? "Auto keeps the load gate on" : null;
        },
        changes: function(group, d5) {
          const by = /* @__PURE__ */ new Map();
          for (const f4 of edited(group, d5)) {
            const k5 = tkey(f4.target);
            if (!by.has(k5)) by.set(k5, {
              target: f4.target.target,
              policy_schema: schemaFor(f4.target, f4.field),
              expected_revision: f4.target.current_revision,
              changes: {}
            });
            const c4 = by.get(k5).changes;
            if (f4.field === "runtime_limit") c4.runtime_limit = d5.values[f4.key] === "" ? null : Number(d5.values[f4.key]);
            else if (f4.field === "mode") c4.mode = d5.values[f4.key];
            else if (f4.field === "maximum") c4.maximum = d5.values[f4.key] === "" ? null : Number(d5.values[f4.key]);
            else {
              const g5 = fields(group).filter((x5) => x5.field === "load_gate");
              const v4 = (x5) => d5.touched[x5.key] ? d5.values[x5.key] : shown(x5);
              c4.load_gate = { enabled: !!v4(g5[0]), cpu_max: Number(v4(g5[1])), memory_min: Number(v4(g5[2])) };
            }
          }
          return [...by.values()];
        },
        review: function(group, d5) {
          return edited(group, d5).map((f4) => f4.label + " " + said(f4) + " \u2192 " + word(f4, d5.values[f4.key]));
        },
        fresh: function() {
          return { draft: null, op: null, receipt: null };
        },
        status: function(group, local, cap, now) {
          if (local.receipt?.state === "saved" && (schemaOf(scopes(group).host, "taskq.host-capacity/1") || (local.op?.ops || []).some((o4) => o4.policy_schema === "taskq.project-limits/1"))) {
            const revision = local.receipt.saved_revision;
            if (typeof revision !== "string") return "unknown";
            const targets = (local.op?.ops || []).map((op) => ({ op, target: (group.targets || []).find((t4) => tkey(t4) === tkey({ target: op.target })) }));
            if (!targets.length || targets.some((x5) => !x5.target) || !freshness(group, now).fresh) return "saved";
            if (targets.every((x5) => x5.target.current_revision === revision && x5.target.effective_revision === revision)) return "applied";
            return targets.some((x5) => ![revision, x5.op.expected_revision].includes(x5.target.current_revision)) ? "superseded" : "saved";
          }
          if (local.receipt && local.receipt.state === "saved") return applied(group, now) === "applied" && JSON.stringify(revs(group)) !== JSON.stringify(local.receipt.from) ? "applied" : "saved";
          if (local.receipt) return local.receipt.state;
          if (!local.draft) return cap && cap.edit && fields(group).length ? "view" : "read-only";
          if (!local.draft.rebind && JSON.stringify(local.draft.base) !== JSON.stringify(revs(group))) return "conflict";
          return local.draft.review ? "review" : "editing";
        },
        step: function(group, local, cap, ev, newId, now) {
          const l5 = { ...local };
          if (l5.draft?.rebind && JSON.stringify(l5.draft.base) !== JSON.stringify(revs(group))) {
            l5.draft = { ...l5.draft, base: revs(group), values: Object.fromEntries(fields(group).map((f4) => [f4.key, l5.draft.touched[f4.key] ? l5.draft.values[f4.key] : shown(f4)])), rebind: false };
          }
          const st = status(group, l5, cap, now);
          switch (ev.type) {
            case "open":
              if (!["view", "applied", "rejected", "superseded"].includes(st)) return { local, send: null };
              return { local: { draft: { base: revs(group), values: Object.fromEntries(fields(group).map((f4) => [f4.key, shown(f4)])), touched: {}, review: false, error: null }, op: null, receipt: null }, send: null };
            case "input":
              if (!l5.draft || ["sending"].includes(st)) return { local, send: null };
              l5.draft = { ...l5.draft, values: { ...l5.draft.values, [ev.key]: ev.value }, touched: { ...l5.draft.touched, [ev.key]: true }, review: false, error: null };
              return { local: l5, send: null };
            case "review":
              if (st !== "editing") return { local, send: null };
              l5.draft = { ...l5.draft, error: problem(group, l5.draft) };
              l5.draft.review = !l5.draft.error;
              return { local: l5, send: null };
            case "back":
              if (st === "review") l5.draft = { ...l5.draft, review: false };
              return { local: l5, send: null };
            case "rebase":
              if (st === "conflict") l5.draft = { ...l5.draft, base: revs(group), values: Object.fromEntries(fields(group).map((f4) => [f4.key, l5.draft.touched[f4.key] ? l5.draft.values[f4.key] : shown(f4)])), review: false };
              return { local: l5, send: null };
            case "cancel":
              return ["editing", "review", "conflict"].includes(st) ? { local: fresh(), send: null } : { local, send: null };
            case "confirm": {
              if (st !== "review") return { local, send: null };
              l5.op = { id: newId(), ops: changes(group, l5.draft) };
              l5.receipt = { state: "sending", from: revs(group), draft: { ...l5.draft, review: false } };
              l5.draft = null;
              return { local: l5, send: l5.op };
            }
            case "receipt": {
              if (!l5.op || ev.opId !== l5.op.id) return { local, send: null };
              const d5 = l5.receipt?.draft, changed = JSON.stringify(l5.receipt?.from) !== JSON.stringify(revs(group));
              if (ev.state === "rejected" && d5 && (l5.op.ops || []).some((o4) => o4.policy_schema === "taskq.host-capacity/1") && (/host policy changed|revision/i.test(ev.reason || "") || changed)) {
                return { local: { draft: { base: revs(group), values: Object.fromEntries(fields(group).map((f4) => [f4.key, d5.touched[f4.key] ? d5.values[f4.key] : shown(f4)])), touched: d5.touched, review: false, error: ev.reason || RECEIPT.conflict, rebind: !changed }, op: null, receipt: null }, send: null };
              }
              l5.receipt = { ...l5.receipt, state: ["saved", "rejected", "unknown"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason, saved_revision: ev.saved_revision };
              return { local: l5, send: null };
            }
            case "retry":
              return st === "unknown" ? { local: { ...l5, receipt: { ...l5.receipt, state: "sending", reason: null } }, send: l5.op } : { local, send: null };
            case "dismiss":
              return ["applied", "rejected", "superseded"].includes(st) ? { local: fresh(), send: null } : { local, send: null };
            default:
              return { local, send: null };
          }
        },
        render: function(group, local, cap, now) {
          if (local.draft?.rebind && JSON.stringify(local.draft.base) !== JSON.stringify(revs(group))) {
            local = { ...local, draft: { ...local.draft, base: revs(group), values: Object.fromEntries(fields(group).map((f5) => [f5.key, local.draft.touched[f5.key] ? local.draft.values[f5.key] : shown(f5)])), rebind: false } };
          }
          const st = status(group, local, cap, now), f4 = freshness(group, now), s4 = scopes(group), id = "rsc-" + String(group.key || "").replace(/[^\w-]/g, "_");
          let h5 = '<section class="rsc" data-rsc-state="' + st + '" aria-label="' + esc("Capacity of " + (schemaOf(s4.host, "taskq.host-capacity/1") ? "all registered projects" : group.labels?.project || "project") + " on " + (group.labels?.host || "this host")) + '">';
          h5 += '<p class="rsc-row"><span>Policy</span><span class="rsc-muted">' + esc(policy(group)) + "</span></p>";
          for (const u4 of usage(group, now)) h5 += '<p class="rsc-row' + (u4.over ? " rsc-over" : "") + '" aria-label="' + esc(rtName(u4.runtime) + ": " + (u4.used == null ? "unknown" : u4.used) + " used of " + u4.limit + (u4.over ? ", over limit; running tasks continue" : "")) + '"><span>' + esc(rtName(u4.runtime)) + '</span><span class="rsc-count">' + esc(u4.text) + "</span></p>";
          h5 += '<p class="rsc-row"><span>Load gate</span><span class="rsc-muted">' + esc(gate(group, now)) + "</span></p>";
          const sm = s4.observation && s4.observation.sample;
          if (sm && (sm.cpu_busy_percent != null || sm.memory_available_percent != null)) {
            const parts = [sm.cpu_busy_percent != null ? "CPU " + Math.round(sm.cpu_busy_percent) + " %" : "CPU unknown", sm.memory_available_percent != null ? "available memory " + Math.round(sm.memory_available_percent) + " %" : "memory unknown"];
            h5 += '<p class="rsc-muted">' + esc(parts.join(" \xB7 ") + (f4.sample != null ? " \xB7 " + (f4.sample > FRESH ? "last sample " + age(f4.sample) + " ago" : age(f4.sample) + " ago") : "")) + "</p>";
          }
          const other = (s4.observation && s4.observation.blockers || []).filter((b5) => !GATE.includes(b5.code) && b5.code !== "sample" && b5.code !== "config");
          if (other.length) h5 += '<p class="rsc-muted">Also blocked: ' + esc(other.map((b5) => BLOCKERS[b5.code] || BLOCKERS.unknown).join(", ")) + "</p>";
          if (s4.observation && s4.observation.completeness !== "complete") h5 += '<p class="rsc-muted">Blockers ' + esc(s4.observation.completeness === "partial" ? "partly unknown" : "unknown") + "</p>";
          if (schemaOf(s4.host, "taskq.host-capacity/1")) {
            const limits = s4.project?.current_policy?.limits;
            h5 += '<p class="rsc-muted">Project ceilings (' + esc(group.labels?.project || "project") + "): " + esc(limits == null ? "unknown" : Object.entries(limits).map(([rt, n3]) => rtName(rt) + " " + n3).join(", ") || "none") + "</p>";
          }
          if (cap && cap.note) h5 += '<p class="rsc-alert">' + esc(cap.note) + "</p>";
          if (st === "view" || st === "read-only") {
            const a4 = applied(group, now);
            if (a4 === "saved") h5 += '<p class="rsc-muted" role="status">' + RECEIPT.saved + "</p>";
            return h5 + (st === "view" ? '<button type="button" data-rsc="open">Edit limits</button>' : cap && cap.reason ? '<p class="rsc-muted">' + esc(cap.reason) + "</p>" : "") + "</section>";
          }
          if (["sending", "saved", "applied", "rejected", "unknown", "superseded"].includes(st)) {
            const r4 = local.receipt;
            h5 += '<p class="rsc-receipt rsc-' + st + '" role="status" aria-live="polite">' + esc(RECEIPT[st] + (r4 && r4.reason ? ": " + r4.reason : "")) + "</p>";
            if (st === "unknown") h5 += '<button type="button" data-rsc="retry">Check again</button>';
            if (st === "applied" || st === "rejected" || st === "superseded") h5 += '<button type="button" data-rsc="open">Edit limits</button>';
            return h5 + "</section>";
          }
          const d5 = local.draft;
          if (st === "review") return h5 + '<div class="rsc-review" role="alert"><p>' + (schemaOf(s4.host, "taskq.host-capacity/1") ? "Apply host limits to all registered projects on " + esc(group.labels?.host || "this host") : "Apply to " + esc(group.labels?.project || "project") + " on " + esc(group.labels?.host || "this host")) + "?</p>" + review(group, d5).map((x5) => "<p>" + esc(x5) + "</p>").join("") + '<p class="rsc-muted">Running tasks continue. Other hosts and unchanged settings stay.</p></div><div class="rsc-actions"><button type="button" data-rsc="confirm">Confirm</button><button type="button" data-rsc="back">Back</button><button type="button" data-rsc="cancel">Cancel</button></div></section>';
          h5 += '<div class="rsc-form">' + fields(group).map((x5) => input(id, x5, d5.values[x5.key])).join("") + "</div>";
          if (st === "conflict") h5 += '<p class="rsc-alert" role="alert">' + RECEIPT.conflict + "</p>";
          if (d5.error) h5 += '<p class="rsc-alert" role="alert">' + esc(d5.error) + "</p>";
          return h5 + '<div class="rsc-actions">' + (st === "conflict" ? '<button type="button" data-rsc="rebase">Review new values</button>' : '<button type="button" data-rsc="review">Apply\u2026</button>') + '<button type="button" data-rsc="cancel">Cancel</button></div></section>';
        },
        input: function(id, x5, v4, mixed, off) {
          const fid = id + "-" + x5.key.replace(/[^\w-]/g, "_"), dis = off ? " disabled" : "";
          let h5 = '<label for="' + fid + '">' + esc(x5.label) + "</label>";
          if (x5.choice) return h5 + '<select id="' + fid + '" data-rsc-field="' + esc(x5.key) + '"' + dis + ">" + (mixed ? '<option value=""' + (v4 === "" ? " selected" : "") + ">Mixed</option>" : "") + x5.choice.map((c4) => '<option value="' + c4 + '"' + (v4 === c4 ? " selected" : "") + ">" + MODES[c4] + "</option>").join("") + "</select>";
          if (x5.bool) return h5 + '<input id="' + fid + '" type="checkbox" role="switch" data-rsc-field="' + esc(x5.key) + '"' + (v4 ? " checked" : "") + (x5.locked || off ? " disabled" : "") + (x5.locked ? ' aria-describedby="' + fid + '-why"' : "") + (mixed ? ' aria-label="' + esc(x5.label) + ': Mixed"' : "") + ">" + (x5.locked ? '<span class="rsc-why" id="' + fid + '-why">Auto keeps the load gate on</span>' : mixed ? '<span class="rsc-why">Mixed</span>' : "");
          return h5 + '<input id="' + fid + '" type="number" inputmode="numeric" step="1" min="0" data-rsc-field="' + esc(x5.key) + '" value="' + esc(v4) + '"' + dis + (mixed ? ' placeholder="Mixed"' : x5.value == null ? ' placeholder="' + esc(x5.none || "none") + '"' : "") + ">";
        },
        mount: function(el, adapter) {
          const apply = function(ev) {
            const r4 = step(group, local, adapter.capability(group), ev, newId, now());
            local = r4.local;
            if (ev.type !== "input") paint();
            if (r4.send) ui.effect("resource.send", () => adapter.send(group, r4.send), (x5) => apply({ type: "receipt", opId: r4.send.id, ...x5 }));
          }, paint = function() {
            const a4 = document.activeElement, focus = a4 && el.contains(a4) ? { sel: a4.dataset.rscField ? '[data-rsc-field="' + a4.dataset.rscField + '"]' : '[data-rsc="' + a4.dataset.rsc + '"]', s: a4.type === "number" ? null : a4.selectionStart, e: a4.selectionEnd } : null;
            el.innerHTML = render(group, local, adapter.capability(group), now());
            const t4 = focus && el.querySelector(focus.sel);
            if (t4) {
              t4.focus();
              if (focus.s != null && t4.setSelectionRange) try {
                t4.setSelectionRange(focus.s, focus.e);
              } catch (_5) {
              }
            }
          };
          let group = null, local = fresh();
          const now = () => adapter.now ? adapter.now() : Math.floor(Date.now() / 1e3);
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "rsc-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e4) => {
            const b5 = e4.target.closest("button[data-rsc]");
            if (b5 && !b5.disabled) apply({ type: b5.dataset.rsc });
          });
          ui.listen(el, "input", (e4) => {
            const k5 = e4.target.dataset.rscField;
            if (k5) apply({ type: "input", key: k5, value: e4.target.type === "checkbox" ? e4.target.checked : e4.target.value });
          });
          return execution.module("resource-controls.mount.controller", {
            update(g5) {
              group = g5;
              if (local.draft?.rebind && JSON.stringify(local.draft.base) !== JSON.stringify(revs(group))) {
                local = { ...local, draft: { ...local.draft, base: revs(group), values: Object.fromEntries(fields(group).map((f4) => [f4.key, local.draft.touched[f4.key] ? local.draft.values[f4.key] : shown(f4)])), rebind: false } };
              }
              paint();
            },
            editing() {
              return !!local.draft;
            }
          });
        },
        workFields: function(groups) {
          const by = /* @__PURE__ */ new Map();
          for (const g5 of groups) for (const f4 of fields(g5)) {
            const x5 = by.get(f4.key) || { ...f4, values: [], locked: false };
            x5.values.push(shown(f4));
            x5.locked = x5.locked || !!f4.locked;
            by.set(f4.key, x5);
          }
          return [...by.values()].map((f4) => ({ ...f4, mixed: new Set(f4.values.map(String)).size > 1 || f4.values.length < groups.length })).sort((a4, b5) => RANK(a4.key) - RANK(b5.key));
        },
        freshWork: function(selected) {
          return { selected: { ...selected }, draft: null, locals: {} };
        },
        workStatus: function(groups, w5, cap, now) {
          const sel = picked(groups, w5), sent = sel.filter((g5) => w5.locals[g5.key]);
          if (sent.length) return { state: "receipts", each: Object.fromEntries(sent.map((g5) => [g5.key, status(g5, w5.locals[g5.key], cap(g5), now)])) };
          if (sel.some((g5) => schemaOf(scopes(g5).host, "taskq.host-capacity/1"))) return { state: "read-only" };
          if (!w5.draft) return { state: sel.length && sel.every((g5) => cap(g5) && cap(g5).edit) ? "view" : "read-only" };
          if (JSON.stringify(w5.draft.base) !== JSON.stringify(workRevs(sel))) return { state: "conflict" };
          return { state: w5.draft.review ? "review" : "editing" };
        },
        workStep: function(groups, w5, cap, ev, newId, now) {
          const st = workStatus(groups, w5, cap, now).state, l5 = { ...w5 }, none = { local: w5, send: [] };
          const values = (gs) => Object.fromEntries(workFields(gs).map((f4) => [f4.key, f4.mixed ? f4.bool ? false : "" : f4.values[0]]));
          switch (ev.type) {
            case "select": {
              if (!["view", "read-only", "editing"].includes(st)) return none;
              l5.selected = { ...w5.selected, [ev.key]: !!ev.on };
              if (l5.draft) {
                const gs = picked(groups, l5);
                l5.draft = { ...l5.draft, base: workRevs(gs), values: { ...values(gs), ...Object.fromEntries(Object.keys(l5.draft.touched).map((k5) => [k5, l5.draft.values[k5]])) }, error: null };
              }
              return { local: l5, send: [] };
            }
            case "open":
              if (st !== "view") return none;
              {
                const gs = picked(groups, w5);
                l5.draft = { base: workRevs(gs), values: values(gs), touched: {}, review: false, error: null };
              }
              return { local: l5, send: [] };
            case "input":
              if (!l5.draft) return none;
              l5.draft = { ...l5.draft, values: { ...l5.draft.values, [ev.key]: ev.value }, touched: { ...l5.draft.touched, [ev.key]: true }, review: false, error: null };
              return { local: l5, send: [] };
            case "review": {
              if (st !== "editing") return none;
              const errs = picked(groups, w5).map((g5) => [g5, problem(g5, l5.draft)]), bad = errs.find(([, e4]) => e4 && e4 !== "No change");
              l5.draft = { ...l5.draft, error: bad ? (bad[0].labels && bad[0].labels.project || bad[0].key) + ": " + bad[1] : errs.some(([, e4]) => !e4) ? null : "No change" };
              l5.draft.review = !l5.draft.error;
              return { local: l5, send: [] };
            }
            case "back":
              if (st === "review") l5.draft = { ...l5.draft, review: false };
              return { local: l5, send: [] };
            case "rebase":
              if (st === "conflict") l5.draft = { ...l5.draft, base: workRevs(picked(groups, w5)), review: false };
              return { local: l5, send: [] };
            case "cancel":
              return ["editing", "review", "conflict"].includes(st) ? { local: freshWork(w5.selected), send: [] } : none;
            case "confirm": {
              if (st !== "review") return none;
              const send = [];
              l5.locals = {};
              for (const g5 of picked(groups, w5)) {
                const ops = changes(g5, w5.draft);
                if (!ops.length) continue;
                const op = { id: newId(), ops };
                l5.locals[g5.key] = { draft: null, op, receipt: { state: "sending", from: revs(g5) } };
                send.push({ key: g5.key, op });
              }
              l5.draft = null;
              return { local: l5, send };
            }
            case "receipt": {
              const x5 = w5.locals[ev.key];
              if (!x5 || !x5.op || ev.opId !== x5.op.id) return none;
              l5.locals = { ...w5.locals, [ev.key]: { ...x5, receipt: { ...x5.receipt, state: ["saved", "rejected", "unknown"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason } } };
              return { local: l5, send: [] };
            }
            case "retry": {
              const x5 = w5.locals[ev.key], g5 = groups.find((g6) => g6.key === ev.key);
              if (!x5 || !g5 || status(g5, x5, cap(g5), now) !== "unknown") return none;
              l5.locals = { ...w5.locals, [ev.key]: { ...x5, receipt: { ...x5.receipt, state: "sending", reason: null } } };
              return { local: l5, send: [{ key: ev.key, op: x5.op }] };
            }
            case "dismiss": {
              const each = workStatus(groups, w5, cap, now).each || {};
              return Object.values(each).every((s4) => s4 === "applied" || s4 === "rejected") ? { local: freshWork(w5.selected), send: [] } : none;
            }
            default:
              return none;
          }
        },
        workRender: function(groups, w5, cap, now, host) {
          const ws = workStatus(groups, w5, cap, now), st = ws.state, sel = picked(groups, w5), d5 = w5.draft;
          const busy = st === "review" || st === "receipts";
          let h5 = '<section class="rsc rsc-work" data-rsc-state="' + st + '" aria-label="' + esc("Balancing on " + (host || "this host")) + '">';
          h5 += '<p class="rsc-row"><span>Balancing</span><span class="rsc-muted">' + esc((host || "this host") + " \xB7 selected projects") + "</span></p>";
          h5 += '<fieldset class="rsc-scope"><legend>Projects</legend>' + groups.map((g5) => '<label><input type="checkbox" data-rsc-select="' + esc(g5.key) + '"' + (w5.selected[g5.key] ? " checked" : "") + (busy ? " disabled" : "") + ">" + esc(g5.labels && g5.labels.project || g5.key) + "</label>").join("") + '<p class="rsc-muted">Other projects remain unchanged.</p></fieldset>';
          if (sel.length) h5 += '<p class="rsc-muted">' + esc(sel.map((g5) => (g5.labels && g5.labels.project || g5.key) + ": " + policy(g5)).join(" \xB7 ")) + "</p>";
          if (st === "receipts") {
            for (const g5 of sel.filter((g6) => ws.each[g6.key])) {
              const s4 = ws.each[g5.key], r4 = w5.locals[g5.key].receipt;
              h5 += '<p class="rsc-receipt rsc-' + s4 + '" role="status" aria-live="polite">' + esc((g5.labels && g5.labels.project || g5.key) + ": " + RECEIPT[s4] + (r4 && r4.reason ? ": " + r4.reason : "")) + "</p>" + (s4 === "unknown" ? '<button type="button" data-rsc="retry" data-rsc-key="' + esc(g5.key) + '">Check again</button>' : "");
            }
            const done = Object.values(ws.each).every((s4) => s4 === "applied" || s4 === "rejected");
            return h5 + (done ? '<button type="button" data-rsc="dismiss">Done</button>' : "") + "</section>";
          }
          if (st === "view" || st === "read-only") return h5 + '<button type="button" data-rsc="open"' + (st === "view" ? "" : " disabled") + ">Edit Balancing</button>" + (st === "read-only" ? '<p class="rsc-muted">' + (sel.length ? "A selected project is read-only here" : "Select projects to change") + "</p>" : "") + "</section>";
          if (st === "review") return h5 + '<div class="rsc-review" role="alert"><p>' + esc("Apply on " + (host || "this host") + "?") + "</p>" + sel.map((g5) => {
            const r4 = review(g5, d5);
            return "<p>" + esc((g5.labels && g5.labels.project || g5.key) + ": " + (r4.length ? r4.join(", ") : "unchanged")) + "</p>";
          }).join("") + '<p class="rsc-muted">Running tasks continue. Other projects, hosts and unchanged settings stay.</p></div><div class="rsc-actions"><button type="button" data-rsc="confirm">Confirm</button><button type="button" data-rsc="back">Back</button><button type="button" data-rsc="cancel">Cancel</button></div></section>';
          h5 += '<div class="rsc-form">' + workFields(sel).map((f4) => input("rsc-work", f4, d5.values[f4.key], f4.mixed && !d5.touched[f4.key], !sel.length)).join("") + "</div>";
          if (st === "conflict") h5 += '<p class="rsc-alert" role="alert">' + RECEIPT.conflict + "</p>";
          if (d5.error) h5 += '<p class="rsc-alert" role="alert">' + esc(d5.error) + "</p>";
          return h5 + '<div class="rsc-actions">' + (st === "conflict" ? '<button type="button" data-rsc="rebase">Review new values</button>' : '<button type="button" data-rsc="review">Review changes</button>') + '<button type="button" data-rsc="cancel">Cancel</button></div></section>';
        },
        mountWork: function(el, adapter, selected) {
          const apply = function(ev) {
            const r4 = workStep(groups, w5, adapter.capability, ev, newId, now());
            w5 = r4.local;
            if (ev.type !== "input") paint();
            for (const x5 of r4.send) {
              const g5 = groups.find((g6) => g6.key === x5.key);
              ui.effect("resource.work.send", () => adapter.send(g5, x5.op), (y5) => apply({ type: "receipt", key: x5.key, opId: x5.op.id, ...y5 }));
            }
          }, paint = function() {
            const a4 = document.activeElement, sel = a4 && el.contains(a4) ? a4.dataset.rscField ? '[data-rsc-field="' + a4.dataset.rscField + '"]' : a4.dataset.rscSelect ? '[data-rsc-select="' + CSS.escape(a4.dataset.rscSelect) + '"]' : '[data-rsc="' + a4.dataset.rsc + '"]' : null;
            el.innerHTML = workRender(groups, w5, adapter.capability, now(), adapter.host);
            const t4 = sel && el.querySelector(sel);
            if (t4) t4.focus();
          };
          let groups = [], w5 = freshWork(selected || {});
          const now = () => adapter.now ? adapter.now() : Math.floor(Date.now() / 1e3);
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "rsc-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e4) => {
            const b5 = e4.target.closest("button[data-rsc]");
            if (b5 && !b5.disabled) apply({ type: b5.dataset.rsc, key: b5.dataset.rscKey });
          });
          ui.listen(el, "change", (e4) => {
            const k5 = e4.target.dataset.rscSelect;
            if (k5) apply({ type: "select", key: k5, on: e4.target.checked });
          });
          ui.listen(el, "input", (e4) => {
            const k5 = e4.target.dataset.rscField;
            if (k5) apply({ type: "input", key: k5, value: e4.target.type === "checkbox" ? e4.target.checked : e4.target.value });
          });
          return execution.module("resource-controls.mountWork.controller", { update(gs) {
            groups = gs;
            paint();
          }, editing() {
            return !!w5.draft;
          } });
        },
        replayWork: function(fx, capability) {
          let groups = fx.groups, w5 = freshWork(fx.selected || {}), n3 = 0;
          const sent = [];
          const out = [];
          const cap = (g5) => capability(g5) || fx.capability;
          for (const ev of fx.events || []) {
            if (ev.type === "refresh") {
              groups = ev.groups.map((i4) => fx.refresh[i4]);
              continue;
            }
            if (ev.type === "check") {
              const s4 = workStatus(groups, w5, cap, fx.now);
              out.push({ status: s4.state, each: s4.each || {}, html: workRender(groups, w5, cap, fx.now, fx.host), sent: sent.slice() });
              continue;
            }
            const e4 = ev.type === "receipt" ? { ...ev, opId: (sent.filter((x5) => x5.key === ev.key).pop() || { op: {} }).op.id } : ev;
            const r4 = workStep(groups, w5, cap, e4, () => "op-" + ++n3, fx.now);
            w5 = r4.local;
            sent.push(...r4.send);
          }
          return out;
        },
        replay: function(fx, capability) {
          let group = fx.groups[0], local = fresh(), n3 = 0;
          const sent = [];
          const out = [];
          const cap = (g5) => capability(g5) || fx.capability;
          for (const ev of fx.events || []) {
            if (ev.type === "refresh") {
              group = fx.groups[ev.group];
              continue;
            }
            if (ev.type === "check") {
              out.push({ status: status(group, local, cap(group), fx.now), html: render(group, local, cap(group), fx.now), sent: sent.slice(), headline: headline(group, fx.now) });
              continue;
            }
            const r4 = step(group, local, cap(group), ev.type === "receipt" ? { ...ev, opId: sent.length && sent[sent.length - 1].id } : ev, () => "op-" + ++n3, fx.now);
            local = r4.local;
            if (r4.send) sent.push(r4.send);
          }
          return out;
        }
      });
      var root = globalThis;
      var STATES = ["view", "editing", "review", "conflict", "sending", "saved", "applied", "rejected", "unknown", "read-only", "superseded"];
      var FRESH = 60;
      var { esc } = execution.module("resource-controls", { esc: (s4) => String(s4 == null ? "" : s4).replace(/[&<>"']/g, (c4) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c4]) });
      var { rtName } = execution.module("resource-controls", { rtName: (rt) => ({ claude: "Claude", codex: "Codex", "codex-app": "Codex app", antigravity: "Antigravity" })[rt] || rt });
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
      var { tkey } = execution.module("resource-controls", { tkey: (t4) => JSON.stringify([t4.target.installation_id ?? null, t4.target.generation ?? null, t4.target.project_id ?? null, t4.target.runtime_id ?? null]) });
      var { schemaOf } = execution.module("resource-controls", { schemaOf: (x5, s4) => x5 && (x5.supported || []).find((p5) => p5.policy_schema === s4) });
      var { schemaFor } = execution.module("resource-controls", { schemaFor: (x5, field) => {
        const p5 = x5 && (x5.supported || []).find((p6) => p6.fields.includes(field) && p6.scopes.includes(x5.target.runtime_id != null ? "runtime" : x5.target.project_id != null ? "project" : "host"));
        return p5 ? p5.policy_schema : null;
      } });
      var { editable } = execution.module("resource-controls", { editable: (x5, field) => !!schemaFor(x5, field) });
      var MODES = { auto: "Auto", fixed: "Fixed" };
      var { shown } = execution.module("resource-controls", { shown: (f4) => f4.bool ? !!f4.value : f4.value == null ? "" : String(f4.value) });
      var { word } = execution.module("resource-controls", { word: (f4, v4) => f4.bool ? v4 ? "on" : "off" : f4.choice ? MODES[v4] || "unknown" : v4 === "" ? f4.none || "none" : v4 });
      var { said } = execution.module("resource-controls", { said: (f4) => word(f4, shown(f4)) });
      var { revs } = execution.module("resource-controls", { revs: (group) => Object.fromEntries((group.targets || []).map((t4) => [tkey(t4), t4.current_revision])) });
      var RECEIPT = {
        sending: "Saving\u2026",
        saved: "Saved; waiting for service",
        applied: "Applied: read back from the service",
        superseded: "Saved; replaced by a newer command",
        rejected: "Not saved",
        unknown: "Not confirmed; the new limits may not be saved. Check again",
        conflict: "Settings changed since you opened the editor"
      };
      var { RANK } = execution.module("resource-controls", { RANK: (k5) => k5.startsWith("host.") ? 0 : k5.startsWith("rt:") ? 1 : 2 });
      var { picked } = execution.module("resource-controls", { picked: (groups, w5) => groups.filter((g5) => w5.selected[g5.key]) });
      var { workRevs } = execution.module("resource-controls", { workRevs: (gs) => Object.fromEntries(gs.map((g5) => [g5.key, revs(g5)])) });
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

  // src/task-controls.js
  var require_task_controls = __commonJS({
    "src/task-controls.js"(exports, module) {
      "use strict";
      var { execution } = require_ui_runtime();
      var LABELS = { start: "Start", stop: "Stop", later: "Later", retry: "Retry" };
      var ICONS = { start: "play", stop: "square", later: "clock", retry: "rotate-ccw" };
      var { op, render, esc } = execution.module("task-controls", {
        esc: function(s4) {
          return String(s4).replace(/[&<>"']/g, (c4) => "&#" + c4.charCodeAt(0) + ";");
        },
        op: function(row, action, prefix) {
          return { id: prefix + "." + action + "." + row.n + "." + row.revision, task: row.n, revision: row.revision, action };
        },
        render: function(row, { key = "", url = "", board = "unknown", except = [] } = {}) {
          const asset = require_ui_icons2().asset;
          const buttons = (row.controls || []).filter((a4) => LABELS[a4] && !except.includes(a4)).map((a4) => '<button data-a="control" data-control="' + a4 + '" data-key="' + esc(key) + '" class="compact-action" title="' + LABELS[a4] + " #" + row.n + '" aria-label="' + LABELS[a4] + " #" + row.n + '">' + asset(ICONS[a4]) + "</button>");
          if (url) buttons.push('<a href="' + esc(url) + '" data-url="' + esc(url) + '" class="compact-action" title="Close #' + row.n + ' on the board" aria-label="Close #' + row.n + ' on the board">' + asset(board === "gitlab" ? "gitlab" : board === "github" ? "github" : "external-link") + "</a>");
          return buttons.length ? '<span class="row task-controls">' + buttons.join("") + "</span>" : "";
        }
      });
      module.exports = { VERSION: "taskq.task-controls/1", LABELS, op, render };
    }
  });

  // node_modules/prismjs/components/prism-core.js
  var require_prism_core = __commonJS({
    "node_modules/prismjs/components/prism-core.js"(exports, module) {
      var _self = typeof window !== "undefined" ? window : typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope ? self : {};
      var Prism2 = (function(_self2) {
        var lang = /(?:^|\s)lang(?:uage)?-([\w-]+)(?=\s|$)/i;
        var uniqueId = 0;
        var plainTextGrammar = {};
        var _5 = {
          /**
           * By default, Prism will attempt to highlight all code elements (by calling {@link Prism.highlightAll}) on the
           * current page after the page finished loading. This might be a problem if e.g. you wanted to asynchronously load
           * additional languages or plugins yourself.
           *
           * By setting this value to `true`, Prism will not automatically highlight all code elements on the page.
           *
           * You obviously have to change this value before the automatic highlighting started. To do this, you can add an
           * empty Prism object into the global scope before loading the Prism script like this:
           *
           * ```js
           * window.Prism = window.Prism || {};
           * Prism.manual = true;
           * // add a new <script> to load Prism's script
           * ```
           *
           * @default false
           * @type {boolean}
           * @memberof Prism
           * @public
           */
          manual: _self2.Prism && _self2.Prism.manual,
          /**
           * By default, if Prism is in a web worker, it assumes that it is in a worker it created itself, so it uses
           * `addEventListener` to communicate with its parent instance. However, if you're using Prism manually in your
           * own worker, you don't want it to do this.
           *
           * By setting this value to `true`, Prism will not add its own listeners to the worker.
           *
           * You obviously have to change this value before Prism executes. To do this, you can add an
           * empty Prism object into the global scope before loading the Prism script like this:
           *
           * ```js
           * window.Prism = window.Prism || {};
           * Prism.disableWorkerMessageHandler = true;
           * // Load Prism's script
           * ```
           *
           * @default false
           * @type {boolean}
           * @memberof Prism
           * @public
           */
          disableWorkerMessageHandler: _self2.Prism && _self2.Prism.disableWorkerMessageHandler,
          /**
           * A namespace for utility methods.
           *
           * All function in this namespace that are not explicitly marked as _public_ are for __internal use only__ and may
           * change or disappear at any time.
           *
           * @namespace
           * @memberof Prism
           */
          util: {
            encode: function encode(tokens) {
              if (tokens instanceof Token) {
                return new Token(tokens.type, encode(tokens.content), tokens.alias);
              } else if (Array.isArray(tokens)) {
                return tokens.map(encode);
              } else {
                return tokens.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\u00a0/g, " ");
              }
            },
            /**
             * Returns the name of the type of the given value.
             *
             * @param {any} o
             * @returns {string}
             * @example
             * type(null)      === 'Null'
             * type(undefined) === 'Undefined'
             * type(123)       === 'Number'
             * type('foo')     === 'String'
             * type(true)      === 'Boolean'
             * type([1, 2])    === 'Array'
             * type({})        === 'Object'
             * type(String)    === 'Function'
             * type(/abc+/)    === 'RegExp'
             */
            type: function(o4) {
              return Object.prototype.toString.call(o4).slice(8, -1);
            },
            /**
             * Returns a unique number for the given object. Later calls will still return the same number.
             *
             * @param {Object} obj
             * @returns {number}
             */
            objId: function(obj) {
              if (!obj["__id"]) {
                Object.defineProperty(obj, "__id", { value: ++uniqueId });
              }
              return obj["__id"];
            },
            /**
             * Creates a deep clone of the given object.
             *
             * The main intended use of this function is to clone language definitions.
             *
             * @param {T} o
             * @param {Record<number, any>} [visited]
             * @returns {T}
             * @template T
             */
            clone: function deepClone(o4, visited) {
              visited = visited || {};
              var clone;
              var id;
              switch (_5.util.type(o4)) {
                case "Object":
                  id = _5.util.objId(o4);
                  if (visited[id]) {
                    return visited[id];
                  }
                  clone = /** @type {Record<string, any>} */
                  {};
                  visited[id] = clone;
                  for (var key in o4) {
                    if (o4.hasOwnProperty(key)) {
                      clone[key] = deepClone(o4[key], visited);
                    }
                  }
                  return (
                    /** @type {any} */
                    clone
                  );
                case "Array":
                  id = _5.util.objId(o4);
                  if (visited[id]) {
                    return visited[id];
                  }
                  clone = [];
                  visited[id] = clone;
                  /** @type {Array} */
                  /** @type {any} */
                  o4.forEach(function(v4, i4) {
                    clone[i4] = deepClone(v4, visited);
                  });
                  return (
                    /** @type {any} */
                    clone
                  );
                default:
                  return o4;
              }
            },
            /**
             * Returns the Prism language of the given element set by a `language-xxxx` or `lang-xxxx` class.
             *
             * If no language is set for the element or the element is `null` or `undefined`, `none` will be returned.
             *
             * @param {Element} element
             * @returns {string}
             */
            getLanguage: function(element) {
              while (element) {
                var m5 = lang.exec(element.className);
                if (m5) {
                  return m5[1].toLowerCase();
                }
                element = element.parentElement;
              }
              return "none";
            },
            /**
             * Sets the Prism `language-xxxx` class of the given element.
             *
             * @param {Element} element
             * @param {string} language
             * @returns {void}
             */
            setLanguage: function(element, language) {
              element.className = element.className.replace(RegExp(lang, "gi"), "");
              element.classList.add("language-" + language);
            },
            /**
             * Returns the script element that is currently executing.
             *
             * This does __not__ work for line script element.
             *
             * @returns {HTMLScriptElement | null}
             */
            currentScript: function() {
              if (typeof document === "undefined") {
                return null;
              }
              if (document.currentScript && document.currentScript.tagName === "SCRIPT" && 1 < 2) {
                return (
                  /** @type {any} */
                  document.currentScript
                );
              }
              try {
                throw new Error();
              } catch (err) {
                var src = (/at [^(\r\n]*\((.*):[^:]+:[^:]+\)$/i.exec(err.stack) || [])[1];
                if (src) {
                  var scripts = document.getElementsByTagName("script");
                  for (var i4 in scripts) {
                    if (scripts[i4].src == src) {
                      return scripts[i4];
                    }
                  }
                }
                return null;
              }
            },
            /**
             * Returns whether a given class is active for `element`.
             *
             * The class can be activated if `element` or one of its ancestors has the given class and it can be deactivated
             * if `element` or one of its ancestors has the negated version of the given class. The _negated version_ of the
             * given class is just the given class with a `no-` prefix.
             *
             * Whether the class is active is determined by the closest ancestor of `element` (where `element` itself is
             * closest ancestor) that has the given class or the negated version of it. If neither `element` nor any of its
             * ancestors have the given class or the negated version of it, then the default activation will be returned.
             *
             * In the paradoxical situation where the closest ancestor contains __both__ the given class and the negated
             * version of it, the class is considered active.
             *
             * @param {Element} element
             * @param {string} className
             * @param {boolean} [defaultActivation=false]
             * @returns {boolean}
             */
            isActive: function(element, className, defaultActivation) {
              var no = "no-" + className;
              while (element) {
                var classList = element.classList;
                if (classList.contains(className)) {
                  return true;
                }
                if (classList.contains(no)) {
                  return false;
                }
                element = element.parentElement;
              }
              return !!defaultActivation;
            }
          },
          /**
           * This namespace contains all currently loaded languages and the some helper functions to create and modify languages.
           *
           * @namespace
           * @memberof Prism
           * @public
           */
          languages: {
            /**
             * The grammar for plain, unformatted text.
             */
            plain: plainTextGrammar,
            plaintext: plainTextGrammar,
            text: plainTextGrammar,
            txt: plainTextGrammar,
            /**
             * Creates a deep copy of the language with the given id and appends the given tokens.
             *
             * If a token in `redef` also appears in the copied language, then the existing token in the copied language
             * will be overwritten at its original position.
             *
             * ## Best practices
             *
             * Since the position of overwriting tokens (token in `redef` that overwrite tokens in the copied language)
             * doesn't matter, they can technically be in any order. However, this can be confusing to others that trying to
             * understand the language definition because, normally, the order of tokens matters in Prism grammars.
             *
             * Therefore, it is encouraged to order overwriting tokens according to the positions of the overwritten tokens.
             * Furthermore, all non-overwriting tokens should be placed after the overwriting ones.
             *
             * @param {string} id The id of the language to extend. This has to be a key in `Prism.languages`.
             * @param {Grammar} redef The new tokens to append.
             * @returns {Grammar} The new language created.
             * @public
             * @example
             * Prism.languages['css-with-colors'] = Prism.languages.extend('css', {
             *     // Prism.languages.css already has a 'comment' token, so this token will overwrite CSS' 'comment' token
             *     // at its original position
             *     'comment': { ... },
             *     // CSS doesn't have a 'color' token, so this token will be appended
             *     'color': /\b(?:red|green|blue)\b/
             * });
             */
            extend: function(id, redef) {
              var lang2 = _5.util.clone(_5.languages[id]);
              for (var key in redef) {
                lang2[key] = redef[key];
              }
              return lang2;
            },
            /**
             * Inserts tokens _before_ another token in a language definition or any other grammar.
             *
             * ## Usage
             *
             * This helper method makes it easy to modify existing languages. For example, the CSS language definition
             * not only defines CSS highlighting for CSS documents, but also needs to define highlighting for CSS embedded
             * in HTML through `<style>` elements. To do this, it needs to modify `Prism.languages.markup` and add the
             * appropriate tokens. However, `Prism.languages.markup` is a regular JavaScript object literal, so if you do
             * this:
             *
             * ```js
             * Prism.languages.markup.style = {
             *     // token
             * };
             * ```
             *
             * then the `style` token will be added (and processed) at the end. `insertBefore` allows you to insert tokens
             * before existing tokens. For the CSS example above, you would use it like this:
             *
             * ```js
             * Prism.languages.insertBefore('markup', 'cdata', {
             *     'style': {
             *         // token
             *     }
             * });
             * ```
             *
             * ## Special cases
             *
             * If the grammars of `inside` and `insert` have tokens with the same name, the tokens in `inside`'s grammar
             * will be ignored.
             *
             * This behavior can be used to insert tokens after `before`:
             *
             * ```js
             * Prism.languages.insertBefore('markup', 'comment', {
             *     'comment': Prism.languages.markup.comment,
             *     // tokens after 'comment'
             * });
             * ```
             *
             * ## Limitations
             *
             * The main problem `insertBefore` has to solve is iteration order. Since ES2015, the iteration order for object
             * properties is guaranteed to be the insertion order (except for integer keys) but some browsers behave
             * differently when keys are deleted and re-inserted. So `insertBefore` can't be implemented by temporarily
             * deleting properties which is necessary to insert at arbitrary positions.
             *
             * To solve this problem, `insertBefore` doesn't actually insert the given tokens into the target object.
             * Instead, it will create a new object and replace all references to the target object with the new one. This
             * can be done without temporarily deleting properties, so the iteration order is well-defined.
             *
             * However, only references that can be reached from `Prism.languages` or `insert` will be replaced. I.e. if
             * you hold the target object in a variable, then the value of the variable will not change.
             *
             * ```js
             * var oldMarkup = Prism.languages.markup;
             * var newMarkup = Prism.languages.insertBefore('markup', 'comment', { ... });
             *
             * assert(oldMarkup !== Prism.languages.markup);
             * assert(newMarkup === Prism.languages.markup);
             * ```
             *
             * @param {string} inside The property of `root` (e.g. a language id in `Prism.languages`) that contains the
             * object to be modified.
             * @param {string} before The key to insert before.
             * @param {Grammar} insert An object containing the key-value pairs to be inserted.
             * @param {Object<string, any>} [root] The object containing `inside`, i.e. the object that contains the
             * object to be modified.
             *
             * Defaults to `Prism.languages`.
             * @returns {Grammar} The new grammar object.
             * @public
             */
            insertBefore: function(inside, before, insert, root) {
              root = root || /** @type {any} */
              _5.languages;
              var grammar = root[inside];
              var ret = {};
              for (var token in grammar) {
                if (grammar.hasOwnProperty(token)) {
                  if (token == before) {
                    for (var newToken in insert) {
                      if (insert.hasOwnProperty(newToken)) {
                        ret[newToken] = insert[newToken];
                      }
                    }
                  }
                  if (!insert.hasOwnProperty(token)) {
                    ret[token] = grammar[token];
                  }
                }
              }
              var old = root[inside];
              root[inside] = ret;
              _5.languages.DFS(_5.languages, function(key, value) {
                if (value === old && key != inside) {
                  this[key] = ret;
                }
              });
              return ret;
            },
            // Traverse a language definition with Depth First Search
            DFS: function DFS(o4, callback, type, visited) {
              visited = visited || {};
              var objId = _5.util.objId;
              for (var i4 in o4) {
                if (o4.hasOwnProperty(i4)) {
                  callback.call(o4, i4, o4[i4], type || i4);
                  var property = o4[i4];
                  var propertyType = _5.util.type(property);
                  if (propertyType === "Object" && !visited[objId(property)]) {
                    visited[objId(property)] = true;
                    DFS(property, callback, null, visited);
                  } else if (propertyType === "Array" && !visited[objId(property)]) {
                    visited[objId(property)] = true;
                    DFS(property, callback, i4, visited);
                  }
                }
              }
            }
          },
          plugins: {},
          /**
           * This is the most high-level function in Prism’s API.
           * It fetches all the elements that have a `.language-xxxx` class and then calls {@link Prism.highlightElement} on
           * each one of them.
           *
           * This is equivalent to `Prism.highlightAllUnder(document, async, callback)`.
           *
           * @param {boolean} [async=false] Same as in {@link Prism.highlightAllUnder}.
           * @param {HighlightCallback} [callback] Same as in {@link Prism.highlightAllUnder}.
           * @memberof Prism
           * @public
           */
          highlightAll: function(async, callback) {
            _5.highlightAllUnder(document, async, callback);
          },
          /**
           * Fetches all the descendants of `container` that have a `.language-xxxx` class and then calls
           * {@link Prism.highlightElement} on each one of them.
           *
           * The following hooks will be run:
           * 1. `before-highlightall`
           * 2. `before-all-elements-highlight`
           * 3. All hooks of {@link Prism.highlightElement} for each element.
           *
           * @param {ParentNode} container The root element, whose descendants that have a `.language-xxxx` class will be highlighted.
           * @param {boolean} [async=false] Whether each element is to be highlighted asynchronously using Web Workers.
           * @param {HighlightCallback} [callback] An optional callback to be invoked on each element after its highlighting is done.
           * @memberof Prism
           * @public
           */
          highlightAllUnder: function(container, async, callback) {
            var env = {
              callback,
              container,
              selector: 'code[class*="language-"], [class*="language-"] code, code[class*="lang-"], [class*="lang-"] code'
            };
            _5.hooks.run("before-highlightall", env);
            env.elements = Array.prototype.slice.apply(env.container.querySelectorAll(env.selector));
            _5.hooks.run("before-all-elements-highlight", env);
            for (var i4 = 0, element; element = env.elements[i4++]; ) {
              _5.highlightElement(element, async === true, env.callback);
            }
          },
          /**
           * Highlights the code inside a single element.
           *
           * The following hooks will be run:
           * 1. `before-sanity-check`
           * 2. `before-highlight`
           * 3. All hooks of {@link Prism.highlight}. These hooks will be run by an asynchronous worker if `async` is `true`.
           * 4. `before-insert`
           * 5. `after-highlight`
           * 6. `complete`
           *
           * Some the above hooks will be skipped if the element doesn't contain any text or there is no grammar loaded for
           * the element's language.
           *
           * @param {Element} element The element containing the code.
           * It must have a class of `language-xxxx` to be processed, where `xxxx` is a valid language identifier.
           * @param {boolean} [async=false] Whether the element is to be highlighted asynchronously using Web Workers
           * to improve performance and avoid blocking the UI when highlighting very large chunks of code. This option is
           * [disabled by default](https://prismjs.com/faq.html#why-is-asynchronous-highlighting-disabled-by-default).
           *
           * Note: All language definitions required to highlight the code must be included in the main `prism.js` file for
           * asynchronous highlighting to work. You can build your own bundle on the
           * [Download page](https://prismjs.com/download.html).
           * @param {HighlightCallback} [callback] An optional callback to be invoked after the highlighting is done.
           * Mostly useful when `async` is `true`, since in that case, the highlighting is done asynchronously.
           * @memberof Prism
           * @public
           */
          highlightElement: function(element, async, callback) {
            var language = _5.util.getLanguage(element);
            var grammar = _5.languages[language];
            _5.util.setLanguage(element, language);
            var parent = element.parentElement;
            if (parent && parent.nodeName.toLowerCase() === "pre") {
              _5.util.setLanguage(parent, language);
            }
            var code = element.textContent;
            var env = {
              element,
              language,
              grammar,
              code
            };
            function insertHighlightedCode(highlightedCode) {
              env.highlightedCode = highlightedCode;
              _5.hooks.run("before-insert", env);
              env.element.innerHTML = env.highlightedCode;
              _5.hooks.run("after-highlight", env);
              _5.hooks.run("complete", env);
              callback && callback.call(env.element);
            }
            _5.hooks.run("before-sanity-check", env);
            parent = env.element.parentElement;
            if (parent && parent.nodeName.toLowerCase() === "pre" && !parent.hasAttribute("tabindex")) {
              parent.setAttribute("tabindex", "0");
            }
            if (!env.code) {
              _5.hooks.run("complete", env);
              callback && callback.call(env.element);
              return;
            }
            _5.hooks.run("before-highlight", env);
            if (!env.grammar) {
              insertHighlightedCode(_5.util.encode(env.code));
              return;
            }
            if (async && _self2.Worker) {
              var worker = new Worker(_5.filename);
              worker.onmessage = function(evt) {
                insertHighlightedCode(evt.data);
              };
              worker.postMessage(JSON.stringify({
                language: env.language,
                code: env.code,
                immediateClose: true
              }));
            } else {
              insertHighlightedCode(_5.highlight(env.code, env.grammar, env.language));
            }
          },
          /**
           * Low-level function, only use if you know what you’re doing. It accepts a string of text as input
           * and the language definitions to use, and returns a string with the HTML produced.
           *
           * The following hooks will be run:
           * 1. `before-tokenize`
           * 2. `after-tokenize`
           * 3. `wrap`: On each {@link Token}.
           *
           * @param {string} text A string with the code to be highlighted.
           * @param {Grammar} grammar An object containing the tokens to use.
           *
           * Usually a language definition like `Prism.languages.markup`.
           * @param {string} language The name of the language definition passed to `grammar`.
           * @returns {string} The highlighted HTML.
           * @memberof Prism
           * @public
           * @example
           * Prism.highlight('var foo = true;', Prism.languages.javascript, 'javascript');
           */
          highlight: function(text, grammar, language) {
            var env = {
              code: text,
              grammar,
              language
            };
            _5.hooks.run("before-tokenize", env);
            if (!env.grammar) {
              throw new Error('The language "' + env.language + '" has no grammar.');
            }
            env.tokens = _5.tokenize(env.code, env.grammar);
            _5.hooks.run("after-tokenize", env);
            return Token.stringify(_5.util.encode(env.tokens), env.language);
          },
          /**
           * This is the heart of Prism, and the most low-level function you can use. It accepts a string of text as input
           * and the language definitions to use, and returns an array with the tokenized code.
           *
           * When the language definition includes nested tokens, the function is called recursively on each of these tokens.
           *
           * This method could be useful in other contexts as well, as a very crude parser.
           *
           * @param {string} text A string with the code to be highlighted.
           * @param {Grammar} grammar An object containing the tokens to use.
           *
           * Usually a language definition like `Prism.languages.markup`.
           * @returns {TokenStream} An array of strings and tokens, a token stream.
           * @memberof Prism
           * @public
           * @example
           * let code = `var foo = 0;`;
           * let tokens = Prism.tokenize(code, Prism.languages.javascript);
           * tokens.forEach(token => {
           *     if (token instanceof Prism.Token && token.type === 'number') {
           *         console.log(`Found numeric literal: ${token.content}`);
           *     }
           * });
           */
          tokenize: function(text, grammar) {
            var rest = grammar.rest;
            if (rest) {
              for (var token in rest) {
                grammar[token] = rest[token];
              }
              delete grammar.rest;
            }
            var tokenList = new LinkedList();
            addAfter(tokenList, tokenList.head, text);
            matchGrammar(text, tokenList, grammar, tokenList.head, 0);
            return toArray(tokenList);
          },
          /**
           * @namespace
           * @memberof Prism
           * @public
           */
          hooks: {
            all: {},
            /**
             * Adds the given callback to the list of callbacks for the given hook.
             *
             * The callback will be invoked when the hook it is registered for is run.
             * Hooks are usually directly run by a highlight function but you can also run hooks yourself.
             *
             * One callback function can be registered to multiple hooks and the same hook multiple times.
             *
             * @param {string} name The name of the hook.
             * @param {HookCallback} callback The callback function which is given environment variables.
             * @public
             */
            add: function(name, callback) {
              var hooks = _5.hooks.all;
              hooks[name] = hooks[name] || [];
              hooks[name].push(callback);
            },
            /**
             * Runs a hook invoking all registered callbacks with the given environment variables.
             *
             * Callbacks will be invoked synchronously and in the order in which they were registered.
             *
             * @param {string} name The name of the hook.
             * @param {Object<string, any>} env The environment variables of the hook passed to all callbacks registered.
             * @public
             */
            run: function(name, env) {
              var callbacks = _5.hooks.all[name];
              if (!callbacks || !callbacks.length) {
                return;
              }
              for (var i4 = 0, callback; callback = callbacks[i4++]; ) {
                callback(env);
              }
            }
          },
          Token
        };
        _self2.Prism = _5;
        function Token(type, content, alias, matchedStr) {
          this.type = type;
          this.content = content;
          this.alias = alias;
          this.length = (matchedStr || "").length | 0;
        }
        Token.stringify = function stringify(o4, language) {
          if (typeof o4 == "string") {
            return o4;
          }
          if (Array.isArray(o4)) {
            var s4 = "";
            o4.forEach(function(e4) {
              s4 += stringify(e4, language);
            });
            return s4;
          }
          var env = {
            type: o4.type,
            content: stringify(o4.content, language),
            tag: "span",
            classes: ["token", o4.type],
            attributes: {},
            language
          };
          var aliases = o4.alias;
          if (aliases) {
            if (Array.isArray(aliases)) {
              Array.prototype.push.apply(env.classes, aliases);
            } else {
              env.classes.push(aliases);
            }
          }
          _5.hooks.run("wrap", env);
          var attributes = "";
          for (var name in env.attributes) {
            attributes += " " + name + '="' + (env.attributes[name] || "").replace(/"/g, "&quot;") + '"';
          }
          return "<" + env.tag + ' class="' + env.classes.join(" ") + '"' + attributes + ">" + env.content + "</" + env.tag + ">";
        };
        function matchPattern(pattern, pos, text, lookbehind) {
          pattern.lastIndex = pos;
          var match = pattern.exec(text);
          if (match && lookbehind && match[1]) {
            var lookbehindLength = match[1].length;
            match.index += lookbehindLength;
            match[0] = match[0].slice(lookbehindLength);
          }
          return match;
        }
        function matchGrammar(text, tokenList, grammar, startNode, startPos, rematch) {
          for (var token in grammar) {
            if (!grammar.hasOwnProperty(token) || !grammar[token]) {
              continue;
            }
            var patterns = grammar[token];
            patterns = Array.isArray(patterns) ? patterns : [patterns];
            for (var j4 = 0; j4 < patterns.length; ++j4) {
              if (rematch && rematch.cause == token + "," + j4) {
                return;
              }
              var patternObj = patterns[j4];
              var inside = patternObj.inside;
              var lookbehind = !!patternObj.lookbehind;
              var greedy = !!patternObj.greedy;
              var alias = patternObj.alias;
              if (greedy && !patternObj.pattern.global) {
                var flags = patternObj.pattern.toString().match(/[imsuy]*$/)[0];
                patternObj.pattern = RegExp(patternObj.pattern.source, flags + "g");
              }
              var pattern = patternObj.pattern || patternObj;
              for (var currentNode = startNode.next, pos = startPos; currentNode !== tokenList.tail; pos += currentNode.value.length, currentNode = currentNode.next) {
                if (rematch && pos >= rematch.reach) {
                  break;
                }
                var str = currentNode.value;
                if (tokenList.length > text.length) {
                  return;
                }
                if (str instanceof Token) {
                  continue;
                }
                var removeCount = 1;
                var match;
                if (greedy) {
                  match = matchPattern(pattern, pos, text, lookbehind);
                  if (!match || match.index >= text.length) {
                    break;
                  }
                  var from = match.index;
                  var to = match.index + match[0].length;
                  var p5 = pos;
                  p5 += currentNode.value.length;
                  while (from >= p5) {
                    currentNode = currentNode.next;
                    p5 += currentNode.value.length;
                  }
                  p5 -= currentNode.value.length;
                  pos = p5;
                  if (currentNode.value instanceof Token) {
                    continue;
                  }
                  for (var k5 = currentNode; k5 !== tokenList.tail && (p5 < to || typeof k5.value === "string"); k5 = k5.next) {
                    removeCount++;
                    p5 += k5.value.length;
                  }
                  removeCount--;
                  str = text.slice(pos, p5);
                  match.index -= pos;
                } else {
                  match = matchPattern(pattern, 0, str, lookbehind);
                  if (!match) {
                    continue;
                  }
                }
                var from = match.index;
                var matchStr = match[0];
                var before = str.slice(0, from);
                var after = str.slice(from + matchStr.length);
                var reach = pos + str.length;
                if (rematch && reach > rematch.reach) {
                  rematch.reach = reach;
                }
                var removeFrom = currentNode.prev;
                if (before) {
                  removeFrom = addAfter(tokenList, removeFrom, before);
                  pos += before.length;
                }
                removeRange(tokenList, removeFrom, removeCount);
                var wrapped = new Token(token, inside ? _5.tokenize(matchStr, inside) : matchStr, alias, matchStr);
                currentNode = addAfter(tokenList, removeFrom, wrapped);
                if (after) {
                  addAfter(tokenList, currentNode, after);
                }
                if (removeCount > 1) {
                  var nestedRematch = {
                    cause: token + "," + j4,
                    reach
                  };
                  matchGrammar(text, tokenList, grammar, currentNode.prev, pos, nestedRematch);
                  if (rematch && nestedRematch.reach > rematch.reach) {
                    rematch.reach = nestedRematch.reach;
                  }
                }
              }
            }
          }
        }
        function LinkedList() {
          var head = { value: null, prev: null, next: null };
          var tail = { value: null, prev: head, next: null };
          head.next = tail;
          this.head = head;
          this.tail = tail;
          this.length = 0;
        }
        function addAfter(list, node, value) {
          var next = node.next;
          var newNode = { value, prev: node, next };
          node.next = newNode;
          next.prev = newNode;
          list.length++;
          return newNode;
        }
        function removeRange(list, node, count) {
          var next = node.next;
          for (var i4 = 0; i4 < count && next !== list.tail; i4++) {
            next = next.next;
          }
          node.next = next;
          next.prev = node;
          list.length -= i4;
        }
        function toArray(list) {
          var array = [];
          var node = list.head.next;
          while (node !== list.tail) {
            array.push(node.value);
            node = node.next;
          }
          return array;
        }
        if (!_self2.document) {
          if (!_self2.addEventListener) {
            return _5;
          }
          if (!_5.disableWorkerMessageHandler) {
            _self2.addEventListener("message", function(evt) {
              var message = JSON.parse(evt.data);
              var lang2 = message.language;
              var code = message.code;
              var immediateClose = message.immediateClose;
              _self2.postMessage(_5.highlight(code, _5.languages[lang2], lang2));
              if (immediateClose) {
                _self2.close();
              }
            }, false);
          }
          return _5;
        }
        var script = _5.util.currentScript();
        if (script) {
          _5.filename = script.src;
          if (script.hasAttribute("data-manual")) {
            _5.manual = true;
          }
        }
        function highlightAutomaticallyCallback() {
          if (!_5.manual) {
            _5.highlightAll();
          }
        }
        if (!_5.manual) {
          var readyState = document.readyState;
          if (readyState === "loading" || readyState === "interactive" && script && script.defer) {
            document.addEventListener("DOMContentLoaded", highlightAutomaticallyCallback);
          } else {
            if (window.requestAnimationFrame) {
              window.requestAnimationFrame(highlightAutomaticallyCallback);
            } else {
              window.setTimeout(highlightAutomaticallyCallback, 16);
            }
          }
        }
        return _5;
      })(_self);
      if (typeof module !== "undefined" && module.exports) {
        module.exports = Prism2;
      }
      if (typeof global !== "undefined") {
        global.Prism = Prism2;
      }
    }
  });

  // node_modules/prismjs/components/prism-markup.js
  var require_prism_markup = __commonJS({
    "node_modules/prismjs/components/prism-markup.js"() {
      Prism.languages.markup = {
        "comment": {
          pattern: /<!--(?:(?!<!--)[\s\S])*?-->/,
          greedy: true
        },
        "prolog": {
          pattern: /<\?[\s\S]+?\?>/,
          greedy: true
        },
        "doctype": {
          // https://www.w3.org/TR/xml/#NT-doctypedecl
          pattern: /<!DOCTYPE(?:[^>"'[\]]|"[^"]*"|'[^']*')+(?:\[(?:[^<"'\]]|"[^"]*"|'[^']*'|<(?!!--)|<!--(?:[^-]|-(?!->))*-->)*\]\s*)?>/i,
          greedy: true,
          inside: {
            "internal-subset": {
              pattern: /(^[^\[]*\[)[\s\S]+(?=\]>$)/,
              lookbehind: true,
              greedy: true,
              inside: null
              // see below
            },
            "string": {
              pattern: /"[^"]*"|'[^']*'/,
              greedy: true
            },
            "punctuation": /^<!|>$|[[\]]/,
            "doctype-tag": /^DOCTYPE/i,
            "name": /[^\s<>'"]+/
          }
        },
        "cdata": {
          pattern: /<!\[CDATA\[[\s\S]*?\]\]>/i,
          greedy: true
        },
        "tag": {
          pattern: /<\/?(?!\d)[^\s>\/=$<%]+(?:\s(?:\s*[^\s>\/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+(?=[\s>]))|(?=[\s/>])))+)?\s*\/?>/,
          greedy: true,
          inside: {
            "tag": {
              pattern: /^<\/?[^\s>\/]+/,
              inside: {
                "punctuation": /^<\/?/,
                "namespace": /^[^\s>\/:]+:/
              }
            },
            "special-attr": [],
            "attr-value": {
              pattern: /=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+)/,
              inside: {
                "punctuation": [
                  {
                    pattern: /^=/,
                    alias: "attr-equals"
                  },
                  {
                    pattern: /^(\s*)["']|["']$/,
                    lookbehind: true
                  }
                ]
              }
            },
            "punctuation": /\/?>/,
            "attr-name": {
              pattern: /[^\s>\/]+/,
              inside: {
                "namespace": /^[^\s>\/:]+:/
              }
            }
          }
        },
        "entity": [
          {
            pattern: /&[\da-z]{1,8};/i,
            alias: "named-entity"
          },
          /&#x?[\da-f]{1,8};/i
        ]
      };
      Prism.languages.markup["tag"].inside["attr-value"].inside["entity"] = Prism.languages.markup["entity"];
      Prism.languages.markup["doctype"].inside["internal-subset"].inside = Prism.languages.markup;
      Prism.hooks.add("wrap", function(env) {
        if (env.type === "entity") {
          env.attributes["title"] = env.content.replace(/&amp;/, "&");
        }
      });
      Object.defineProperty(Prism.languages.markup.tag, "addInlined", {
        /**
         * Adds an inlined language to markup.
         *
         * An example of an inlined language is CSS with `<style>` tags.
         *
         * @param {string} tagName The name of the tag that contains the inlined language. This name will be treated as
         * case insensitive.
         * @param {string} lang The language key.
         * @example
         * addInlined('style', 'css');
         */
        value: function addInlined(tagName, lang) {
          var includedCdataInside = {};
          includedCdataInside["language-" + lang] = {
            pattern: /(^<!\[CDATA\[)[\s\S]+?(?=\]\]>$)/i,
            lookbehind: true,
            inside: Prism.languages[lang]
          };
          includedCdataInside["cdata"] = /^<!\[CDATA\[|\]\]>$/i;
          var inside = {
            "included-cdata": {
              pattern: /<!\[CDATA\[[\s\S]*?\]\]>/i,
              inside: includedCdataInside
            }
          };
          inside["language-" + lang] = {
            pattern: /[\s\S]+/,
            inside: Prism.languages[lang]
          };
          var def = {};
          def[tagName] = {
            pattern: RegExp(/(<__[^>]*>)(?:<!\[CDATA\[(?:[^\]]|\](?!\]>))*\]\]>|(?!<!\[CDATA\[)[\s\S])*?(?=<\/__>)/.source.replace(/__/g, function() {
              return tagName;
            }), "i"),
            lookbehind: true,
            greedy: true,
            inside
          };
          Prism.languages.insertBefore("markup", "cdata", def);
        }
      });
      Object.defineProperty(Prism.languages.markup.tag, "addAttribute", {
        /**
         * Adds an pattern to highlight languages embedded in HTML attributes.
         *
         * An example of an inlined language is CSS with `style` attributes.
         *
         * @param {string} attrName The name of the tag that contains the inlined language. This name will be treated as
         * case insensitive.
         * @param {string} lang The language key.
         * @example
         * addAttribute('style', 'css');
         */
        value: function(attrName, lang) {
          Prism.languages.markup.tag.inside["special-attr"].push({
            pattern: RegExp(
              /(^|["'\s])/.source + "(?:" + attrName + ")" + /\s*=\s*(?:"[^"]*"|'[^']*'|[^\s'">=]+(?=[\s>]))/.source,
              "i"
            ),
            lookbehind: true,
            inside: {
              "attr-name": /^[^\s=]+/,
              "attr-value": {
                pattern: /=[\s\S]+/,
                inside: {
                  "value": {
                    pattern: /(^=\s*(["']|(?!["'])))\S[\s\S]*(?=\2$)/,
                    lookbehind: true,
                    alias: [lang, "language-" + lang],
                    inside: Prism.languages[lang]
                  },
                  "punctuation": [
                    {
                      pattern: /^=/,
                      alias: "attr-equals"
                    },
                    /"|'/
                  ]
                }
              }
            }
          });
        }
      });
      Prism.languages.html = Prism.languages.markup;
      Prism.languages.mathml = Prism.languages.markup;
      Prism.languages.svg = Prism.languages.markup;
      Prism.languages.xml = Prism.languages.extend("markup", {});
      Prism.languages.ssml = Prism.languages.xml;
      Prism.languages.atom = Prism.languages.xml;
      Prism.languages.rss = Prism.languages.xml;
    }
  });

  // node_modules/prismjs/components/prism-css.js
  var require_prism_css = __commonJS({
    "node_modules/prismjs/components/prism-css.js"() {
      (function(Prism2) {
        var string = /(?:"(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')/;
        Prism2.languages.css = {
          "comment": /\/\*[\s\S]*?\*\//,
          "atrule": {
            pattern: RegExp("@[\\w-](?:" + /[^;{\s"']|\s+(?!\s)/.source + "|" + string.source + ")*?" + /(?:;|(?=\s*\{))/.source),
            inside: {
              "rule": /^@[\w-]+/,
              "selector-function-argument": {
                pattern: /(\bselector\s*\(\s*(?![\s)]))(?:[^()\s]|\s+(?![\s)])|\((?:[^()]|\([^()]*\))*\))+(?=\s*\))/,
                lookbehind: true,
                alias: "selector"
              },
              "keyword": {
                pattern: /(^|[^\w-])(?:and|not|only|or)(?![\w-])/,
                lookbehind: true
              }
              // See rest below
            }
          },
          "url": {
            // https://drafts.csswg.org/css-values-3/#urls
            pattern: RegExp("\\burl\\((?:" + string.source + "|" + /(?:[^\\\r\n()"']|\\[\s\S])*/.source + ")\\)", "i"),
            greedy: true,
            inside: {
              "function": /^url/i,
              "punctuation": /^\(|\)$/,
              "string": {
                pattern: RegExp("^" + string.source + "$"),
                alias: "url"
              }
            }
          },
          "selector": {
            pattern: RegExp(`(^|[{}\\s])[^{}\\s](?:[^{};"'\\s]|\\s+(?![\\s{])|` + string.source + ")*(?=\\s*\\{)"),
            lookbehind: true
          },
          "string": {
            pattern: string,
            greedy: true
          },
          "property": {
            pattern: /(^|[^-\w\xA0-\uFFFF])(?!\s)[-_a-z\xA0-\uFFFF](?:(?!\s)[-\w\xA0-\uFFFF])*(?=\s*:)/i,
            lookbehind: true
          },
          "important": /!important\b/i,
          "function": {
            pattern: /(^|[^-a-z0-9])[-a-z0-9]+(?=\()/i,
            lookbehind: true
          },
          "punctuation": /[(){};:,]/
        };
        Prism2.languages.css["atrule"].inside.rest = Prism2.languages.css;
        var markup = Prism2.languages.markup;
        if (markup) {
          markup.tag.addInlined("style", "css");
          markup.tag.addAttribute("style", "css");
        }
      })(Prism);
    }
  });

  // node_modules/prismjs/components/prism-clike.js
  var require_prism_clike = __commonJS({
    "node_modules/prismjs/components/prism-clike.js"() {
      Prism.languages.clike = {
        "comment": [
          {
            pattern: /(^|[^\\])\/\*[\s\S]*?(?:\*\/|$)/,
            lookbehind: true,
            greedy: true
          },
          {
            pattern: /(^|[^\\:])\/\/.*/,
            lookbehind: true,
            greedy: true
          }
        ],
        "string": {
          pattern: /(["'])(?:\\(?:\r\n|[\s\S])|(?!\1)[^\\\r\n])*\1/,
          greedy: true
        },
        "class-name": {
          pattern: /(\b(?:class|extends|implements|instanceof|interface|new|trait)\s+|\bcatch\s+\()[\w.\\]+/i,
          lookbehind: true,
          inside: {
            "punctuation": /[.\\]/
          }
        },
        "keyword": /\b(?:break|catch|continue|do|else|finally|for|function|if|in|instanceof|new|null|return|throw|try|while)\b/,
        "boolean": /\b(?:false|true)\b/,
        "function": /\b\w+(?=\()/,
        "number": /\b0x[\da-f]+\b|(?:\b\d+(?:\.\d*)?|\B\.\d+)(?:e[+-]?\d+)?/i,
        "operator": /[<>]=?|[!=]=?=?|--?|\+\+?|&&?|\|\|?|[?*/~^%]/,
        "punctuation": /[{}[\];(),.:]/
      };
    }
  });

  // node_modules/prismjs/components/prism-javascript.js
  var require_prism_javascript = __commonJS({
    "node_modules/prismjs/components/prism-javascript.js"() {
      Prism.languages.javascript = Prism.languages.extend("clike", {
        "class-name": [
          Prism.languages.clike["class-name"],
          {
            pattern: /(^|[^$\w\xA0-\uFFFF])(?!\s)[_$A-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\.(?:constructor|prototype))/,
            lookbehind: true
          }
        ],
        "keyword": [
          {
            pattern: /((?:^|\})\s*)catch\b/,
            lookbehind: true
          },
          {
            pattern: /(^|[^.]|\.\.\.\s*)\b(?:as|assert(?=\s*\{)|async(?=\s*(?:function\b|\(|[$\w\xA0-\uFFFF]|$))|await|break|case|class|const|continue|debugger|default|delete|do|else|enum|export|extends|finally(?=\s*(?:\{|$))|for|from(?=\s*(?:['"]|$))|function|(?:get|set)(?=\s*(?:[#\[$\w\xA0-\uFFFF]|$))|if|implements|import|in|instanceof|interface|let|new|null|of|package|private|protected|public|return|static|super|switch|this|throw|try|typeof|undefined|var|void|while|with|yield)\b/,
            lookbehind: true
          }
        ],
        // Allow for all non-ASCII characters (See http://stackoverflow.com/a/2008444)
        "function": /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*(?:\.\s*(?:apply|bind|call)\s*)?\()/,
        "number": {
          pattern: RegExp(
            /(^|[^\w$])/.source + "(?:" + // constant
            (/NaN|Infinity/.source + "|" + // binary integer
            /0[bB][01]+(?:_[01]+)*n?/.source + "|" + // octal integer
            /0[oO][0-7]+(?:_[0-7]+)*n?/.source + "|" + // hexadecimal integer
            /0[xX][\dA-Fa-f]+(?:_[\dA-Fa-f]+)*n?/.source + "|" + // decimal bigint
            /\d+(?:_\d+)*n/.source + "|" + // decimal number (integer or float) but no bigint
            /(?:\d+(?:_\d+)*(?:\.(?:\d+(?:_\d+)*)?)?|\.\d+(?:_\d+)*)(?:[Ee][+-]?\d+(?:_\d+)*)?/.source) + ")" + /(?![\w$])/.source
          ),
          lookbehind: true
        },
        "operator": /--|\+\+|\*\*=?|=>|&&=?|\|\|=?|[!=]==|<<=?|>>>?=?|[-+*/%&|^!=<>]=?|\.{3}|\?\?=?|\?\.?|[~:]/
      });
      Prism.languages.javascript["class-name"][0].pattern = /(\b(?:class|extends|implements|instanceof|interface|new)\s+)[\w.\\]+/;
      Prism.languages.insertBefore("javascript", "keyword", {
        "regex": {
          pattern: RegExp(
            // lookbehind
            // eslint-disable-next-line regexp/no-dupe-characters-character-class
            /((?:^|[^$\w\xA0-\uFFFF."'\])\s]|\b(?:return|yield))\s*)/.source + // Regex pattern:
            // There are 2 regex patterns here. The RegExp set notation proposal added support for nested character
            // classes if the `v` flag is present. Unfortunately, nested CCs are both context-free and incompatible
            // with the only syntax, so we have to define 2 different regex patterns.
            /\//.source + "(?:" + /(?:\[(?:[^\]\\\r\n]|\\.)*\]|\\.|[^/\\\[\r\n])+\/[dgimyus]{0,7}/.source + "|" + // `v` flag syntax. This supports 3 levels of nested character classes.
            /(?:\[(?:[^[\]\\\r\n]|\\.|\[(?:[^[\]\\\r\n]|\\.|\[(?:[^[\]\\\r\n]|\\.)*\])*\])*\]|\\.|[^/\\\[\r\n])+\/[dgimyus]{0,7}v[dgimyus]{0,7}/.source + ")" + // lookahead
            /(?=(?:\s|\/\*(?:[^*]|\*(?!\/))*\*\/)*(?:$|[\r\n,.;:})\]]|\/\/))/.source
          ),
          lookbehind: true,
          greedy: true,
          inside: {
            "regex-source": {
              pattern: /^(\/)[\s\S]+(?=\/[a-z]*$)/,
              lookbehind: true,
              alias: "language-regex",
              inside: Prism.languages.regex
            },
            "regex-delimiter": /^\/|\/$/,
            "regex-flags": /^[a-z]+$/
          }
        },
        // This must be declared before keyword because we use "function" inside the look-forward
        "function-variable": {
          pattern: /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*[=:]\s*(?:async\s*)?(?:\bfunction\b|(?:\((?:[^()]|\([^()]*\))*\)|(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*)\s*=>))/,
          alias: "function"
        },
        "parameter": [
          {
            pattern: /(function(?:\s+(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*)?\s*\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\))/,
            lookbehind: true,
            inside: Prism.languages.javascript
          },
          {
            pattern: /(^|[^$\w\xA0-\uFFFF])(?!\s)[_$a-z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*=>)/i,
            lookbehind: true,
            inside: Prism.languages.javascript
          },
          {
            pattern: /(\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\)\s*=>)/,
            lookbehind: true,
            inside: Prism.languages.javascript
          },
          {
            pattern: /((?:\b|\s|^)(?!(?:as|async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|enum|export|extends|finally|for|from|function|get|if|implements|import|in|instanceof|interface|let|new|null|of|package|private|protected|public|return|set|static|super|switch|this|throw|try|typeof|undefined|var|void|while|with|yield)(?![$\w\xA0-\uFFFF]))(?:(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*\s*)\(\s*|\]\s*\(\s*)(?!\s)(?:[^()\s]|\s+(?![\s)])|\([^()]*\))+(?=\s*\)\s*\{)/,
            lookbehind: true,
            inside: Prism.languages.javascript
          }
        ],
        "constant": /\b[A-Z](?:[A-Z_]|\dx?)*\b/
      });
      Prism.languages.insertBefore("javascript", "string", {
        "hashbang": {
          pattern: /^#!.*/,
          greedy: true,
          alias: "comment"
        },
        "template-string": {
          pattern: /`(?:\\[\s\S]|\$\{(?:[^{}]|\{(?:[^{}]|\{[^}]*\})*\})+\}|(?!\$\{)[^\\`])*`/,
          greedy: true,
          inside: {
            "template-punctuation": {
              pattern: /^`|`$/,
              alias: "string"
            },
            "interpolation": {
              pattern: /((?:^|[^\\])(?:\\{2})*)\$\{(?:[^{}]|\{(?:[^{}]|\{[^}]*\})*\})+\}/,
              lookbehind: true,
              inside: {
                "interpolation-punctuation": {
                  pattern: /^\$\{|\}$/,
                  alias: "punctuation"
                },
                rest: Prism.languages.javascript
              }
            },
            "string": /[\s\S]+/
          }
        },
        "string-property": {
          pattern: /((?:^|[,{])[ \t]*)(["'])(?:\\(?:\r\n|[\s\S])|(?!\2)[^\\\r\n])*\2(?=\s*:)/m,
          lookbehind: true,
          greedy: true,
          alias: "property"
        }
      });
      Prism.languages.insertBefore("javascript", "operator", {
        "literal-property": {
          pattern: /((?:^|[,{])[ \t]*)(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?=\s*:)/m,
          lookbehind: true,
          alias: "property"
        }
      });
      if (Prism.languages.markup) {
        Prism.languages.markup.tag.addInlined("script", "javascript");
        Prism.languages.markup.tag.addAttribute(
          /on(?:abort|blur|change|click|composition(?:end|start|update)|dblclick|error|focus(?:in|out)?|key(?:down|up)|load|mouse(?:down|enter|leave|move|out|over|up)|reset|resize|scroll|select|slotchange|submit|unload|wheel)/.source,
          "javascript"
        );
      }
      Prism.languages.js = Prism.languages.javascript;
    }
  });

  // node_modules/prismjs/components/prism-rust.js
  var require_prism_rust = __commonJS({
    "node_modules/prismjs/components/prism-rust.js"() {
      (function(Prism2) {
        var multilineComment = /\/\*(?:[^*/]|\*(?!\/)|\/(?!\*)|<self>)*\*\//.source;
        for (var i4 = 0; i4 < 2; i4++) {
          multilineComment = multilineComment.replace(/<self>/g, function() {
            return multilineComment;
          });
        }
        multilineComment = multilineComment.replace(/<self>/g, function() {
          return /[^\s\S]/.source;
        });
        Prism2.languages.rust = {
          "comment": [
            {
              pattern: RegExp(/(^|[^\\])/.source + multilineComment),
              lookbehind: true,
              greedy: true
            },
            {
              pattern: /(^|[^\\:])\/\/.*/,
              lookbehind: true,
              greedy: true
            }
          ],
          "string": {
            pattern: /b?"(?:\\[\s\S]|[^\\"])*"|b?r(#*)"(?:[^"]|"(?!\1))*"\1/,
            greedy: true
          },
          "char": {
            pattern: /b?'(?:\\(?:x[0-7][\da-fA-F]|u\{(?:[\da-fA-F]_*){1,6}\}|.)|[^\\\r\n\t'])'/,
            greedy: true
          },
          "attribute": {
            pattern: /#!?\[(?:[^\[\]"]|"(?:\\[\s\S]|[^\\"])*")*\]/,
            greedy: true,
            alias: "attr-name",
            inside: {
              "string": null
              // see below
            }
          },
          // Closure params should not be confused with bitwise OR |
          "closure-params": {
            pattern: /([=(,:]\s*|\bmove\s*)\|[^|]*\||\|[^|]*\|(?=\s*(?:\{|->))/,
            lookbehind: true,
            greedy: true,
            inside: {
              "closure-punctuation": {
                pattern: /^\||\|$/,
                alias: "punctuation"
              },
              rest: null
              // see below
            }
          },
          "lifetime-annotation": {
            pattern: /'\w+/,
            alias: "symbol"
          },
          "fragment-specifier": {
            pattern: /(\$\w+:)[a-z]+/,
            lookbehind: true,
            alias: "punctuation"
          },
          "variable": /\$\w+/,
          "function-definition": {
            pattern: /(\bfn\s+)\w+/,
            lookbehind: true,
            alias: "function"
          },
          "type-definition": {
            pattern: /(\b(?:enum|struct|trait|type|union)\s+)\w+/,
            lookbehind: true,
            alias: "class-name"
          },
          "module-declaration": [
            {
              pattern: /(\b(?:crate|mod)\s+)[a-z][a-z_\d]*/,
              lookbehind: true,
              alias: "namespace"
            },
            {
              pattern: /(\b(?:crate|self|super)\s*)::\s*[a-z][a-z_\d]*\b(?:\s*::(?:\s*[a-z][a-z_\d]*\s*::)*)?/,
              lookbehind: true,
              alias: "namespace",
              inside: {
                "punctuation": /::/
              }
            }
          ],
          "keyword": [
            // https://github.com/rust-lang/reference/blob/master/src/keywords.md
            /\b(?:Self|abstract|as|async|await|become|box|break|const|continue|crate|do|dyn|else|enum|extern|final|fn|for|if|impl|in|let|loop|macro|match|mod|move|mut|override|priv|pub|ref|return|self|static|struct|super|trait|try|type|typeof|union|unsafe|unsized|use|virtual|where|while|yield)\b/,
            // primitives and str
            // https://doc.rust-lang.org/stable/rust-by-example/primitives.html
            /\b(?:bool|char|f(?:32|64)|[ui](?:8|16|32|64|128|size)|str)\b/
          ],
          // functions can technically start with an upper-case letter, but this will introduce a lot of false positives
          // and Rust's naming conventions recommend snake_case anyway.
          // https://doc.rust-lang.org/1.0.0/style/style/naming/README.html
          "function": /\b[a-z_]\w*(?=\s*(?:::\s*<|\())/,
          "macro": {
            pattern: /\b\w+!/,
            alias: "property"
          },
          "constant": /\b[A-Z_][A-Z_\d]+\b/,
          "class-name": /\b[A-Z]\w*\b/,
          "namespace": {
            pattern: /(?:\b[a-z][a-z_\d]*\s*::\s*)*\b[a-z][a-z_\d]*\s*::(?!\s*<)/,
            inside: {
              "punctuation": /::/
            }
          },
          // Hex, oct, bin, dec numbers with visual separators and type suffix
          "number": /\b(?:0x[\dA-Fa-f](?:_?[\dA-Fa-f])*|0o[0-7](?:_?[0-7])*|0b[01](?:_?[01])*|(?:(?:\d(?:_?\d)*)?\.)?\d(?:_?\d)*(?:[Ee][+-]?\d+)?)(?:_?(?:f32|f64|[iu](?:8|16|32|64|size)?))?\b/,
          "boolean": /\b(?:false|true)\b/,
          "punctuation": /->|\.\.=|\.{1,3}|::|[{}[\];(),:]/,
          "operator": /[-+*\/%!^]=?|=[=>]?|&[&=]?|\|[|=]?|<<?=?|>>?=?|[@?]/
        };
        Prism2.languages.rust["closure-params"].inside.rest = Prism2.languages.rust;
        Prism2.languages.rust["attribute"].inside["string"] = Prism2.languages.rust["string"];
      })(Prism);
    }
  });

  // node_modules/prismjs/components/prism-python.js
  var require_prism_python = __commonJS({
    "node_modules/prismjs/components/prism-python.js"() {
      Prism.languages.python = {
        "comment": {
          pattern: /(^|[^\\])#.*/,
          lookbehind: true,
          greedy: true
        },
        "string-interpolation": {
          pattern: /(?:f|fr|rf)(?:("""|''')[\s\S]*?\1|("|')(?:\\.|(?!\2)[^\\\r\n])*\2)/i,
          greedy: true,
          inside: {
            "interpolation": {
              // "{" <expression> <optional "!s", "!r", or "!a"> <optional ":" format specifier> "}"
              pattern: /((?:^|[^{])(?:\{\{)*)\{(?!\{)(?:[^{}]|\{(?!\{)(?:[^{}]|\{(?!\{)(?:[^{}])+\})+\})+\}/,
              lookbehind: true,
              inside: {
                "format-spec": {
                  pattern: /(:)[^:(){}]+(?=\}$)/,
                  lookbehind: true
                },
                "conversion-option": {
                  pattern: /![sra](?=[:}]$)/,
                  alias: "punctuation"
                },
                rest: null
              }
            },
            "string": /[\s\S]+/
          }
        },
        "triple-quoted-string": {
          pattern: /(?:[rub]|br|rb)?("""|''')[\s\S]*?\1/i,
          greedy: true,
          alias: "string"
        },
        "string": {
          pattern: /(?:[rub]|br|rb)?("|')(?:\\.|(?!\1)[^\\\r\n])*\1/i,
          greedy: true
        },
        "function": {
          pattern: /((?:^|\s)def[ \t]+)[a-zA-Z_]\w*(?=\s*\()/g,
          lookbehind: true
        },
        "class-name": {
          pattern: /(\bclass\s+)\w+/i,
          lookbehind: true
        },
        "decorator": {
          pattern: /(^[\t ]*)@\w+(?:\.\w+)*/m,
          lookbehind: true,
          alias: ["annotation", "punctuation"],
          inside: {
            "punctuation": /\./
          }
        },
        "keyword": /\b(?:_(?=\s*:)|and|as|assert|async|await|break|case|class|continue|def|del|elif|else|except|exec|finally|for|from|global|if|import|in|is|lambda|match|nonlocal|not|or|pass|print|raise|return|try|while|with|yield)\b/,
        "builtin": /\b(?:__import__|abs|all|any|apply|ascii|basestring|bin|bool|buffer|bytearray|bytes|callable|chr|classmethod|cmp|coerce|compile|complex|delattr|dict|dir|divmod|enumerate|eval|execfile|file|filter|float|format|frozenset|getattr|globals|hasattr|hash|help|hex|id|input|int|intern|isinstance|issubclass|iter|len|list|locals|long|map|max|memoryview|min|next|object|oct|open|ord|pow|property|range|raw_input|reduce|reload|repr|reversed|round|set|setattr|slice|sorted|staticmethod|str|sum|super|tuple|type|unichr|unicode|vars|xrange|zip)\b/,
        "boolean": /\b(?:False|None|True)\b/,
        "number": /\b0(?:b(?:_?[01])+|o(?:_?[0-7])+|x(?:_?[a-f0-9])+)\b|(?:\b\d+(?:_\d+)*(?:\.(?:\d+(?:_\d+)*)?)?|\B\.\d+(?:_\d+)*)(?:e[+-]?\d+(?:_\d+)*)?j?(?!\w)/i,
        "operator": /[-+%=]=?|!=|:=|\*\*?=?|\/\/?=?|<[<=>]?|>[=>]?|[&|^~]/,
        "punctuation": /[{}[\];(),.:]/
      };
      Prism.languages.python["string-interpolation"].inside["interpolation"].inside.rest = Prism.languages.python;
      Prism.languages.py = Prism.languages.python;
    }
  });

  // node_modules/prismjs/components/prism-bash.js
  var require_prism_bash = __commonJS({
    "node_modules/prismjs/components/prism-bash.js"() {
      (function(Prism2) {
        var envVars = "\\b(?:BASH|BASHOPTS|BASH_ALIASES|BASH_ARGC|BASH_ARGV|BASH_CMDS|BASH_COMPLETION_COMPAT_DIR|BASH_LINENO|BASH_REMATCH|BASH_SOURCE|BASH_VERSINFO|BASH_VERSION|COLORTERM|COLUMNS|COMP_WORDBREAKS|DBUS_SESSION_BUS_ADDRESS|DEFAULTS_PATH|DESKTOP_SESSION|DIRSTACK|DISPLAY|EUID|GDMSESSION|GDM_LANG|GNOME_KEYRING_CONTROL|GNOME_KEYRING_PID|GPG_AGENT_INFO|GROUPS|HISTCONTROL|HISTFILE|HISTFILESIZE|HISTSIZE|HOME|HOSTNAME|HOSTTYPE|IFS|INSTANCE|JOB|LANG|LANGUAGE|LC_ADDRESS|LC_ALL|LC_IDENTIFICATION|LC_MEASUREMENT|LC_MONETARY|LC_NAME|LC_NUMERIC|LC_PAPER|LC_TELEPHONE|LC_TIME|LESSCLOSE|LESSOPEN|LINES|LOGNAME|LS_COLORS|MACHTYPE|MAILCHECK|MANDATORY_PATH|NO_AT_BRIDGE|OLDPWD|OPTERR|OPTIND|ORBIT_SOCKETDIR|OSTYPE|PAPERSIZE|PATH|PIPESTATUS|PPID|PS1|PS2|PS3|PS4|PWD|RANDOM|REPLY|SECONDS|SELINUX_INIT|SESSION|SESSIONTYPE|SESSION_MANAGER|SHELL|SHELLOPTS|SHLVL|SSH_AUTH_SOCK|TERM|UID|UPSTART_EVENTS|UPSTART_INSTANCE|UPSTART_JOB|UPSTART_SESSION|USER|WINDOWID|XAUTHORITY|XDG_CONFIG_DIRS|XDG_CURRENT_DESKTOP|XDG_DATA_DIRS|XDG_GREETER_DATA_DIR|XDG_MENU_PREFIX|XDG_RUNTIME_DIR|XDG_SEAT|XDG_SEAT_PATH|XDG_SESSION_DESKTOP|XDG_SESSION_ID|XDG_SESSION_PATH|XDG_SESSION_TYPE|XDG_VTNR|XMODIFIERS)\\b";
        var commandAfterHeredoc = {
          pattern: /(^(["']?)\w+\2)[ \t]+\S.*/,
          lookbehind: true,
          alias: "punctuation",
          // this looks reasonably well in all themes
          inside: null
          // see below
        };
        var insideString = {
          "bash": commandAfterHeredoc,
          "environment": {
            pattern: RegExp("\\$" + envVars),
            alias: "constant"
          },
          "variable": [
            // [0]: Arithmetic Environment
            {
              pattern: /\$?\(\([\s\S]+?\)\)/,
              greedy: true,
              inside: {
                // If there is a $ sign at the beginning highlight $(( and )) as variable
                "variable": [
                  {
                    pattern: /(^\$\(\([\s\S]+)\)\)/,
                    lookbehind: true
                  },
                  /^\$\(\(/
                ],
                "number": /\b0x[\dA-Fa-f]+\b|(?:\b\d+(?:\.\d*)?|\B\.\d+)(?:[Ee]-?\d+)?/,
                // Operators according to https://www.gnu.org/software/bash/manual/bashref.html#Shell-Arithmetic
                "operator": /--|\+\+|\*\*=?|<<=?|>>=?|&&|\|\||[=!+\-*/%<>^&|]=?|[?~:]/,
                // If there is no $ sign at the beginning highlight (( and )) as punctuation
                "punctuation": /\(\(?|\)\)?|,|;/
              }
            },
            // [1]: Command Substitution
            {
              pattern: /\$\((?:\([^)]+\)|[^()])+\)|`[^`]+`/,
              greedy: true,
              inside: {
                "variable": /^\$\(|^`|\)$|`$/
              }
            },
            // [2]: Brace expansion
            {
              pattern: /\$\{[^}]+\}/,
              greedy: true,
              inside: {
                "operator": /:[-=?+]?|[!\/]|##?|%%?|\^\^?|,,?/,
                "punctuation": /[\[\]]/,
                "environment": {
                  pattern: RegExp("(\\{)" + envVars),
                  lookbehind: true,
                  alias: "constant"
                }
              }
            },
            /\$(?:\w+|[#?*!@$])/
          ],
          // Escape sequences from echo and printf's manuals, and escaped quotes.
          "entity": /\\(?:[abceEfnrtv\\"]|O?[0-7]{1,3}|U[0-9a-fA-F]{8}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{1,2})/
        };
        Prism2.languages.bash = {
          "shebang": {
            pattern: /^#!\s*\/.*/,
            alias: "important"
          },
          "comment": {
            pattern: /(^|[^"{\\$])#.*/,
            lookbehind: true
          },
          "function-name": [
            // a) function foo {
            // b) foo() {
            // c) function foo() {
            // but not “foo {”
            {
              // a) and c)
              pattern: /(\bfunction\s+)[\w-]+(?=(?:\s*\(?:\s*\))?\s*\{)/,
              lookbehind: true,
              alias: "function"
            },
            {
              // b)
              pattern: /\b[\w-]+(?=\s*\(\s*\)\s*\{)/,
              alias: "function"
            }
          ],
          // Highlight variable names as variables in for and select beginnings.
          "for-or-select": {
            pattern: /(\b(?:for|select)\s+)\w+(?=\s+in\s)/,
            alias: "variable",
            lookbehind: true
          },
          // Highlight variable names as variables in the left-hand part
          // of assignments (“=” and “+=”).
          "assign-left": {
            pattern: /(^|[\s;|&]|[<>]\()\w+(?:\.\w+)*(?=\+?=)/,
            inside: {
              "environment": {
                pattern: RegExp("(^|[\\s;|&]|[<>]\\()" + envVars),
                lookbehind: true,
                alias: "constant"
              }
            },
            alias: "variable",
            lookbehind: true
          },
          // Highlight parameter names as variables
          "parameter": {
            pattern: /(^|\s)-{1,2}(?:\w+:[+-]?)?\w+(?:\.\w+)*(?=[=\s]|$)/,
            alias: "variable",
            lookbehind: true
          },
          "string": [
            // Support for Here-documents https://en.wikipedia.org/wiki/Here_document
            {
              pattern: /((?:^|[^<])<<-?\s*)(\w+)\s[\s\S]*?(?:\r?\n|\r)\2/,
              lookbehind: true,
              greedy: true,
              inside: insideString
            },
            // Here-document with quotes around the tag
            // → No expansion (so no “inside”).
            {
              pattern: /((?:^|[^<])<<-?\s*)(["'])(\w+)\2\s[\s\S]*?(?:\r?\n|\r)\3/,
              lookbehind: true,
              greedy: true,
              inside: {
                "bash": commandAfterHeredoc
              }
            },
            // “Normal” string
            {
              // https://www.gnu.org/software/bash/manual/html_node/Double-Quotes.html
              pattern: /(^|[^\\](?:\\\\)*)"(?:\\[\s\S]|\$\([^)]+\)|\$(?!\()|`[^`]+`|[^"\\`$])*"/,
              lookbehind: true,
              greedy: true,
              inside: insideString
            },
            {
              // https://www.gnu.org/software/bash/manual/html_node/Single-Quotes.html
              pattern: /(^|[^$\\])'[^']*'/,
              lookbehind: true,
              greedy: true
            },
            {
              // https://www.gnu.org/software/bash/manual/html_node/ANSI_002dC-Quoting.html
              pattern: /\$'(?:[^'\\]|\\[\s\S])*'/,
              greedy: true,
              inside: {
                "entity": insideString.entity
              }
            }
          ],
          "environment": {
            pattern: RegExp("\\$?" + envVars),
            alias: "constant"
          },
          "variable": insideString.variable,
          "function": {
            pattern: /(^|[\s;|&]|[<>]\()(?:add|apropos|apt|apt-cache|apt-get|aptitude|aspell|automysqlbackup|awk|basename|bash|bc|bconsole|bg|bzip2|cal|cargo|cat|cfdisk|chgrp|chkconfig|chmod|chown|chroot|cksum|clear|cmp|column|comm|composer|cp|cron|crontab|csplit|curl|cut|date|dc|dd|ddrescue|debootstrap|df|diff|diff3|dig|dir|dircolors|dirname|dirs|dmesg|docker|docker-compose|du|egrep|eject|env|ethtool|expand|expect|expr|fdformat|fdisk|fg|fgrep|file|find|fmt|fold|format|free|fsck|ftp|fuser|gawk|git|gparted|grep|groupadd|groupdel|groupmod|groups|grub-mkconfig|gzip|halt|head|hg|history|host|hostname|htop|iconv|id|ifconfig|ifdown|ifup|import|install|ip|java|jobs|join|kill|killall|less|link|ln|locate|logname|logrotate|look|lpc|lpr|lprint|lprintd|lprintq|lprm|ls|lsof|lynx|make|man|mc|mdadm|mkconfig|mkdir|mke2fs|mkfifo|mkfs|mkisofs|mknod|mkswap|mmv|more|most|mount|mtools|mtr|mutt|mv|nano|nc|netstat|nice|nl|node|nohup|notify-send|npm|nslookup|op|open|parted|passwd|paste|pathchk|ping|pkill|pnpm|podman|podman-compose|popd|pr|printcap|printenv|ps|pushd|pv|quota|quotacheck|quotactl|ram|rar|rcp|reboot|remsync|rename|renice|rev|rm|rmdir|rpm|rsync|scp|screen|sdiff|sed|sendmail|seq|service|sftp|sh|shellcheck|shuf|shutdown|sleep|slocate|sort|split|ssh|stat|strace|su|sudo|sum|suspend|swapon|sync|sysctl|tac|tail|tar|tee|time|timeout|top|touch|tr|traceroute|tsort|tty|umount|uname|unexpand|uniq|units|unrar|unshar|unzip|update-grub|uptime|useradd|userdel|usermod|users|uudecode|uuencode|v|vcpkg|vdir|vi|vim|virsh|vmstat|wait|watch|wc|wget|whereis|which|who|whoami|write|xargs|xdg-open|yarn|yes|zenity|zip|zsh|zypper)(?=$|[)\s;|&])/,
            lookbehind: true
          },
          "keyword": {
            pattern: /(^|[\s;|&]|[<>]\()(?:case|do|done|elif|else|esac|fi|for|function|if|in|select|then|until|while)(?=$|[)\s;|&])/,
            lookbehind: true
          },
          // https://www.gnu.org/software/bash/manual/html_node/Shell-Builtin-Commands.html
          "builtin": {
            pattern: /(^|[\s;|&]|[<>]\()(?:\.|:|alias|bind|break|builtin|caller|cd|command|continue|declare|echo|enable|eval|exec|exit|export|getopts|hash|help|let|local|logout|mapfile|printf|pwd|read|readarray|readonly|return|set|shift|shopt|source|test|times|trap|type|typeset|ulimit|umask|unalias|unset)(?=$|[)\s;|&])/,
            lookbehind: true,
            // Alias added to make those easier to distinguish from strings.
            alias: "class-name"
          },
          "boolean": {
            pattern: /(^|[\s;|&]|[<>]\()(?:false|true)(?=$|[)\s;|&])/,
            lookbehind: true
          },
          "file-descriptor": {
            pattern: /\B&\d\b/,
            alias: "important"
          },
          "operator": {
            // Lots of redirections here, but not just that.
            pattern: /\d?<>|>\||\+=|=[=~]?|!=?|<<[<-]?|[&\d]?>>|\d[<>]&?|[<>][&=]?|&[>&]?|\|[&|]?/,
            inside: {
              "file-descriptor": {
                pattern: /^\d/,
                alias: "important"
              }
            }
          },
          "punctuation": /\$?\(\(?|\)\)?|\.\.|[{}[\];\\]/,
          "number": {
            pattern: /(^|\s)(?:[1-9]\d*|0)(?:[.,]\d+)?\b/,
            lookbehind: true
          }
        };
        commandAfterHeredoc.inside = Prism2.languages.bash;
        var toBeCopied = [
          "comment",
          "function-name",
          "for-or-select",
          "assign-left",
          "parameter",
          "string",
          "environment",
          "function",
          "keyword",
          "builtin",
          "boolean",
          "file-descriptor",
          "operator",
          "punctuation",
          "number"
        ];
        var inside = insideString.variable[1].inside;
        for (var i4 = 0; i4 < toBeCopied.length; i4++) {
          inside[toBeCopied[i4]] = Prism2.languages.bash[toBeCopied[i4]];
        }
        Prism2.languages.sh = Prism2.languages.bash;
        Prism2.languages.shell = Prism2.languages.bash;
      })(Prism);
    }
  });

  // node_modules/prismjs/components/prism-json.js
  var require_prism_json = __commonJS({
    "node_modules/prismjs/components/prism-json.js"() {
      Prism.languages.json = {
        "property": {
          pattern: /(^|[^\\])"(?:\\.|[^\\"\r\n])*"(?=\s*:)/,
          lookbehind: true,
          greedy: true
        },
        "string": {
          pattern: /(^|[^\\])"(?:\\.|[^\\"\r\n])*"(?!\s*:)/,
          lookbehind: true,
          greedy: true
        },
        "comment": {
          pattern: /\/\/.*|\/\*[\s\S]*?(?:\*\/|$)/,
          greedy: true
        },
        "number": /-?\b\d+(?:\.\d+)?(?:e[+-]?\d+)?\b/i,
        "punctuation": /[{}[\],]/,
        "operator": /:/,
        "boolean": /\b(?:false|true)\b/,
        "null": {
          pattern: /\bnull\b/,
          alias: "keyword"
        }
      };
      Prism.languages.webmanifest = Prism.languages.json;
    }
  });

  // node_modules/prismjs/components/prism-typescript.js
  var require_prism_typescript = __commonJS({
    "node_modules/prismjs/components/prism-typescript.js"() {
      (function(Prism2) {
        Prism2.languages.typescript = Prism2.languages.extend("javascript", {
          "class-name": {
            pattern: /(\b(?:class|extends|implements|instanceof|interface|new|type)\s+)(?!keyof\b)(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*(?:\s*<(?:[^<>]|<(?:[^<>]|<[^<>]*>)*>)*>)?/,
            lookbehind: true,
            greedy: true,
            inside: null
            // see below
          },
          "builtin": /\b(?:Array|Function|Promise|any|boolean|console|never|number|string|symbol|unknown)\b/
        });
        Prism2.languages.typescript.keyword.push(
          /\b(?:abstract|declare|is|keyof|readonly|require)\b/,
          // keywords that have to be followed by an identifier
          /\b(?:asserts|infer|interface|module|namespace|type)\b(?=\s*(?:[{_$a-zA-Z\xA0-\uFFFF]|$))/,
          // This is for `import type *, {}`
          /\btype\b(?=\s*(?:[\{*]|$))/
        );
        delete Prism2.languages.typescript["parameter"];
        delete Prism2.languages.typescript["literal-property"];
        var typeInside = Prism2.languages.extend("typescript", {});
        delete typeInside["class-name"];
        Prism2.languages.typescript["class-name"].inside = typeInside;
        Prism2.languages.insertBefore("typescript", "function", {
          "decorator": {
            pattern: /@[$\w\xA0-\uFFFF]+/,
            inside: {
              "at": {
                pattern: /^@/,
                alias: "operator"
              },
              "function": /^[\s\S]+/
            }
          },
          "generic-function": {
            // e.g. foo<T extends "bar" | "baz">( ...
            pattern: /#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*\s*<(?:[^<>]|<(?:[^<>]|<[^<>]*>)*>)*>(?=\s*\()/,
            greedy: true,
            inside: {
              "function": /^#?(?!\s)[_$a-zA-Z\xA0-\uFFFF](?:(?!\s)[$\w\xA0-\uFFFF])*/,
              "generic": {
                pattern: /<[\s\S]+/,
                // everything after the first <
                alias: "class-name",
                inside: typeInside
              }
            }
          }
        });
        Prism2.languages.ts = Prism2.languages.typescript;
      })(Prism);
    }
  });

  // node_modules/prismjs/components/prism-diff.js
  var require_prism_diff = __commonJS({
    "node_modules/prismjs/components/prism-diff.js"() {
      (function(Prism2) {
        Prism2.languages.diff = {
          "coord": [
            // Match all kinds of coord lines (prefixed by "+++", "---" or "***").
            /^(?:\*{3}|-{3}|\+{3}).*$/m,
            // Match "@@ ... @@" coord lines in unified diff.
            /^@@.*@@$/m,
            // Match coord lines in normal diff (starts with a number).
            /^\d.*$/m
          ]
          // deleted, inserted, unchanged, diff
        };
        var PREFIXES = {
          "deleted-sign": "-",
          "deleted-arrow": "<",
          "inserted-sign": "+",
          "inserted-arrow": ">",
          "unchanged": " ",
          "diff": "!"
        };
        Object.keys(PREFIXES).forEach(function(name) {
          var prefix = PREFIXES[name];
          var alias = [];
          if (!/^\w+$/.test(name)) {
            alias.push(/\w+/.exec(name)[0]);
          }
          if (name === "diff") {
            alias.push("bold");
          }
          Prism2.languages.diff[name] = {
            pattern: RegExp("^(?:[" + prefix + "].*(?:\r\n?|\n|(?![\\s\\S])))+", "m"),
            alias,
            inside: {
              "line": {
                pattern: /(.)(?=[\s\S]).*(?:\r\n?|\n)?/,
                lookbehind: true
              },
              "prefix": {
                pattern: /[\s\S]/,
                alias: /\w+/.exec(name)[0]
              }
            }
          };
        });
        Object.defineProperty(Prism2.languages.diff, "PREFIXES", {
          value: PREFIXES
        });
      })(Prism);
    }
  });

  // node_modules/prismjs/components/prism-sql.js
  var require_prism_sql = __commonJS({
    "node_modules/prismjs/components/prism-sql.js"() {
      Prism.languages.sql = {
        "comment": {
          pattern: /(^|[^\\])(?:\/\*[\s\S]*?\*\/|(?:--|\/\/|#).*)/,
          lookbehind: true
        },
        "variable": [
          {
            pattern: /@(["'`])(?:\\[\s\S]|(?!\1)[^\\])+\1/,
            greedy: true
          },
          /@[\w.$]+/
        ],
        "string": {
          pattern: /(^|[^@\\])("|')(?:\\[\s\S]|(?!\2)[^\\]|\2\2)*\2/,
          greedy: true,
          lookbehind: true
        },
        "identifier": {
          pattern: /(^|[^@\\])`(?:\\[\s\S]|[^`\\]|``)*`/,
          greedy: true,
          lookbehind: true,
          inside: {
            "punctuation": /^`|`$/
          }
        },
        "function": /\b(?:AVG|COUNT|FIRST|FORMAT|LAST|LCASE|LEN|MAX|MID|MIN|MOD|NOW|ROUND|SUM|UCASE)(?=\s*\()/i,
        // Should we highlight user defined functions too?
        "keyword": /\b(?:ACTION|ADD|AFTER|ALGORITHM|ALL|ALTER|ANALYZE|ANY|APPLY|AS|ASC|AUTHORIZATION|AUTO_INCREMENT|BACKUP|BDB|BEGIN|BERKELEYDB|BIGINT|BINARY|BIT|BLOB|BOOL|BOOLEAN|BREAK|BROWSE|BTREE|BULK|BY|CALL|CASCADED?|CASE|CHAIN|CHAR(?:ACTER|SET)?|CHECK(?:POINT)?|CLOSE|CLUSTERED|COALESCE|COLLATE|COLUMNS?|COMMENT|COMMIT(?:TED)?|COMPUTE|CONNECT|CONSISTENT|CONSTRAINT|CONTAINS(?:TABLE)?|CONTINUE|CONVERT|CREATE|CROSS|CURRENT(?:_DATE|_TIME|_TIMESTAMP|_USER)?|CURSOR|CYCLE|DATA(?:BASES?)?|DATE(?:TIME)?|DAY|DBCC|DEALLOCATE|DEC|DECIMAL|DECLARE|DEFAULT|DEFINER|DELAYED|DELETE|DELIMITERS?|DENY|DESC|DESCRIBE|DETERMINISTIC|DISABLE|DISCARD|DISK|DISTINCT|DISTINCTROW|DISTRIBUTED|DO|DOUBLE|DROP|DUMMY|DUMP(?:FILE)?|DUPLICATE|ELSE(?:IF)?|ENABLE|ENCLOSED|END|ENGINE|ENUM|ERRLVL|ERRORS|ESCAPED?|EXCEPT|EXEC(?:UTE)?|EXISTS|EXIT|EXPLAIN|EXTENDED|FETCH|FIELDS|FILE|FILLFACTOR|FIRST|FIXED|FLOAT|FOLLOWING|FOR(?: EACH ROW)?|FORCE|FOREIGN|FREETEXT(?:TABLE)?|FROM|FULL|FUNCTION|GEOMETRY(?:COLLECTION)?|GLOBAL|GOTO|GRANT|GROUP|HANDLER|HASH|HAVING|HOLDLOCK|HOUR|IDENTITY(?:COL|_INSERT)?|IF|IGNORE|IMPORT|INDEX|INFILE|INNER|INNODB|INOUT|INSERT|INT|INTEGER|INTERSECT|INTERVAL|INTO|INVOKER|ISOLATION|ITERATE|JOIN|KEYS?|KILL|LANGUAGE|LAST|LEAVE|LEFT|LEVEL|LIMIT|LINENO|LINES|LINESTRING|LOAD|LOCAL|LOCK|LONG(?:BLOB|TEXT)|LOOP|MATCH(?:ED)?|MEDIUM(?:BLOB|INT|TEXT)|MERGE|MIDDLEINT|MINUTE|MODE|MODIFIES|MODIFY|MONTH|MULTI(?:LINESTRING|POINT|POLYGON)|NATIONAL|NATURAL|NCHAR|NEXT|NO|NONCLUSTERED|NULLIF|NUMERIC|OFF?|OFFSETS?|ON|OPEN(?:DATASOURCE|QUERY|ROWSET)?|OPTIMIZE|OPTION(?:ALLY)?|ORDER|OUT(?:ER|FILE)?|OVER|PARTIAL|PARTITION|PERCENT|PIVOT|PLAN|POINT|POLYGON|PRECEDING|PRECISION|PREPARE|PREV|PRIMARY|PRINT|PRIVILEGES|PROC(?:EDURE)?|PUBLIC|PURGE|QUICK|RAISERROR|READS?|REAL|RECONFIGURE|REFERENCES|RELEASE|RENAME|REPEAT(?:ABLE)?|REPLACE|REPLICATION|REQUIRE|RESIGNAL|RESTORE|RESTRICT|RETURN(?:ING|S)?|REVOKE|RIGHT|ROLLBACK|ROUTINE|ROW(?:COUNT|GUIDCOL|S)?|RTREE|RULE|SAVE(?:POINT)?|SCHEMA|SECOND|SELECT|SERIAL(?:IZABLE)?|SESSION(?:_USER)?|SET(?:USER)?|SHARE|SHOW|SHUTDOWN|SIMPLE|SMALLINT|SNAPSHOT|SOME|SONAME|SQL|START(?:ING)?|STATISTICS|STATUS|STRIPED|SYSTEM_USER|TABLES?|TABLESPACE|TEMP(?:ORARY|TABLE)?|TERMINATED|TEXT(?:SIZE)?|THEN|TIME(?:STAMP)?|TINY(?:BLOB|INT|TEXT)|TOP?|TRAN(?:SACTIONS?)?|TRIGGER|TRUNCATE|TSEQUAL|TYPES?|UNBOUNDED|UNCOMMITTED|UNDEFINED|UNION|UNIQUE|UNLOCK|UNPIVOT|UNSIGNED|UPDATE(?:TEXT)?|USAGE|USE|USER|USING|VALUES?|VAR(?:BINARY|CHAR|CHARACTER|YING)|VIEW|WAITFOR|WARNINGS|WHEN|WHERE|WHILE|WITH(?: ROLLUP|IN)?|WORK|WRITE(?:TEXT)?|YEAR)\b/i,
        "boolean": /\b(?:FALSE|NULL|TRUE)\b/i,
        "number": /\b0x[\da-f]+\b|\b\d+(?:\.\d*)?|\B\.\d+\b/i,
        "operator": /[-+*\/=%^~]|&&?|\|\|?|!=?|<(?:=>?|<|>)?|>[>=]?|\b(?:AND|BETWEEN|DIV|ILIKE|IN|IS|LIKE|NOT|OR|REGEXP|RLIKE|SOUNDS LIKE|XOR)\b/i,
        "punctuation": /[;[\]()`,.]/
      };
    }
  });

  // node_modules/prismjs/components/prism-yaml.js
  var require_prism_yaml = __commonJS({
    "node_modules/prismjs/components/prism-yaml.js"() {
      (function(Prism2) {
        var anchorOrAlias = /[*&][^\s[\]{},]+/;
        var tag = /!(?:<[\w\-%#;/?:@&=+$,.!~*'()[\]]+>|(?:[a-zA-Z\d-]*!)?[\w\-%#;/?:@&=+$.~*'()]+)?/;
        var properties = "(?:" + tag.source + "(?:[ 	]+" + anchorOrAlias.source + ")?|" + anchorOrAlias.source + "(?:[ 	]+" + tag.source + ")?)";
        var plainKey = /(?:[^\s\x00-\x08\x0e-\x1f!"#%&'*,\-:>?@[\]`{|}\x7f-\x84\x86-\x9f\ud800-\udfff\ufffe\uffff]|[?:-]<PLAIN>)(?:[ \t]*(?:(?![#:])<PLAIN>|:<PLAIN>))*/.source.replace(/<PLAIN>/g, function() {
          return /[^\s\x00-\x08\x0e-\x1f,[\]{}\x7f-\x84\x86-\x9f\ud800-\udfff\ufffe\uffff]/.source;
        });
        var string = /"(?:[^"\\\r\n]|\\.)*"|'(?:[^'\\\r\n]|\\.)*'/.source;
        function createValuePattern(value, flags) {
          flags = (flags || "").replace(/m/g, "") + "m";
          var pattern = /([:\-,[{]\s*(?:\s<<prop>>[ \t]+)?)(?:<<value>>)(?=[ \t]*(?:$|,|\]|\}|(?:[\r\n]\s*)?#))/.source.replace(/<<prop>>/g, function() {
            return properties;
          }).replace(/<<value>>/g, function() {
            return value;
          });
          return RegExp(pattern, flags);
        }
        Prism2.languages.yaml = {
          "scalar": {
            pattern: RegExp(/([\-:]\s*(?:\s<<prop>>[ \t]+)?[|>])[ \t]*(?:((?:\r?\n|\r)[ \t]+)\S[^\r\n]*(?:\2[^\r\n]+)*)/.source.replace(/<<prop>>/g, function() {
              return properties;
            })),
            lookbehind: true,
            alias: "string"
          },
          "comment": /#.*/,
          "key": {
            pattern: RegExp(/((?:^|[:\-,[{\r\n?])[ \t]*(?:<<prop>>[ \t]+)?)<<key>>(?=\s*:\s)/.source.replace(/<<prop>>/g, function() {
              return properties;
            }).replace(/<<key>>/g, function() {
              return "(?:" + plainKey + "|" + string + ")";
            })),
            lookbehind: true,
            greedy: true,
            alias: "atrule"
          },
          "directive": {
            pattern: /(^[ \t]*)%.+/m,
            lookbehind: true,
            alias: "important"
          },
          "datetime": {
            pattern: createValuePattern(/\d{4}-\d\d?-\d\d?(?:[tT]|[ \t]+)\d\d?:\d{2}:\d{2}(?:\.\d*)?(?:[ \t]*(?:Z|[-+]\d\d?(?::\d{2})?))?|\d{4}-\d{2}-\d{2}|\d\d?:\d{2}(?::\d{2}(?:\.\d*)?)?/.source),
            lookbehind: true,
            alias: "number"
          },
          "boolean": {
            pattern: createValuePattern(/false|true/.source, "i"),
            lookbehind: true,
            alias: "important"
          },
          "null": {
            pattern: createValuePattern(/null|~/.source, "i"),
            lookbehind: true,
            alias: "important"
          },
          "string": {
            pattern: createValuePattern(string),
            lookbehind: true,
            greedy: true
          },
          "number": {
            pattern: createValuePattern(/[+-]?(?:0x[\da-f]+|0o[0-7]+|(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?|\.inf|\.nan)/.source, "i"),
            lookbehind: true
          },
          "tag": tag,
          "important": anchorOrAlias,
          "punctuation": /---|[:[\]{}\-,|>?]|\.\.\./
        };
        Prism2.languages.yml = Prism2.languages.yaml;
      })(Prism);
    }
  });

  // src/reply-files.js
  var require_reply_files = __commonJS({
    "src/reply-files.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var { execution } = ui;
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var waiting = /* @__PURE__ */ new Map();
      var api = execution.module("reply-files", {
        settle: function(id) {
          ui.cancel(waiting.get(id));
          waiting.delete(id);
        },
        problem: function(files) {
          const f4 = (files || []).find((f5) => f5.pending || f5.error);
          return f4 ? f4.error || f4.name + ": copying attachment; wait before Send" : "";
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
            const id = "file-" + Array.from(globalThis.crypto.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join(""), file = { id, name: source.name || "pasted-file", mime: source.type || "application/octet-stream", bytes: source.size };
            put({ ...file, pending: true });
            waiting.set(id, ui.defer(() => {
              waiting.delete(id);
              api.refuse("storage");
              put({ ...file, error: file.name + ": copy receipt missing; remove and select it again before sending" });
            }, 3e4));
            ui.effect("reply.files.read", () => new Promise((resolve, reject) => {
              const r4 = new FileReader();
              ui.listen(r4, "load", () => resolve(r4.result));
              ui.listen(r4, "error", () => reject(Error(file.name + ": file unreadable; select a readable original and retry")));
              r4.readAsDataURL(source);
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
          return { onPaste: (e4) => {
            if (!disabled && e4.clipboardData?.files.length) {
              e4.preventDefault();
              add([...e4.clipboardData.files]);
            }
          }, onDragOver: (e4) => {
            if (!disabled) e4.preventDefault();
          }, onDrop: (e4) => {
            if (!disabled && e4.dataTransfer?.files.length) {
              e4.preventDefault();
              add([...e4.dataTransfer.files]);
            }
          } };
        },
        controls: function(files, disabled, add, remove) {
          return h5("div", { class: "conversation-files" }, (files || []).map((f4) => h5("span", { key: f4.id, class: "conversation-file" }, f4.name + " \xB7 " + f4.bytes + " bytes" + (f4.pending ? " \xB7 Copying\u2026" : f4.error ? " \xB7 Not attached" : ""), h5("button", { type: "button", disabled, "aria-label": "Remove " + f4.name, onClick: () => remove(f4.id) }, "\xD7"))), h5("label", { class: "attach-label", title: "Attach images or files", "aria-disabled": String(disabled) }, "Attach", h5("input", { type: "file", multiple: true, disabled, "data-reply-files": "pick", "aria-label": "Attach images or files", onChange: (e4) => {
            add([...e4.currentTarget.files]);
            e4.currentTarget.value = "";
          } })));
        },
        markup: function(files, disabled) {
          const esc = (s4) => String(s4).replace(/[&<>"']/g, (c4) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c4]);
          return '<div class="conversation-files">' + (files || []).map((f4) => "<span>" + esc(f4.name) + " \xB7 " + f4.bytes + ' bytes <button type="button" data-qac="remove-file" data-file="' + esc(f4.id) + '" aria-label="Remove ' + esc(f4.name) + '"' + (disabled ? " disabled" : "") + ">\xD7</button></span>").join("") + '<label>Attach <input type="file" multiple data-qac="files" aria-label="Attach images or files"' + (disabled ? " disabled" : "") + "></label></div>";
        }
      });
      module.exports = api;
    }
  });

  // src/answer-card.js
  var require_answer_card = __commonJS({
    "src/answer-card.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var replyFiles = require_reply_files();
      var { fresh, textProblem, status, step, render, compose, receipt, mount, replay } = execution.module("answer-card", {
        fresh: function(row) {
          return { draft: "", bound: row.revision, op: null, receipt: null };
        },
        textProblem: function(q4, text, files) {
          if (q4?.resultDate) {
            const at = /* @__PURE__ */ new Date(text + "T12:00:00Z");
            return /^\d{4}-\d{2}-\d{2}$/.test(text) && Number.isFinite(+at) && at.toISOString().slice(0, 10) === text && at > Date.now() ? null : "Choose a future revisit date";
          }
          if (q4?.resultReason && (text.trim().length === 0 || [...text.trim()].length > 180 || /[\r\n]/.test(text))) return "Give a one-line reason, at most 180 characters";
          return replyFiles.problem(files) || (text.trim() || files?.length ? null : "Type an answer");
        },
        status: function(row, local, cap) {
          const q4 = row.question;
          if (row.resultDecision && local.bound !== row.revision) return "stale";
          if (row.resultDecision && row.state === "closed" && q4?.parsed && row.repeatable && local.receipt?.state === "accepted" && cap?.answer) return "actionable";
          if (local.op && local.receipt && (HELD.includes(local.receipt.state) || local.op.revision === row.revision || !q4)) return local.receipt.state;
          if (!manageable(row, cap) && (!q4 || !q4.parsed || !cap || !cap.answer || !(row.resultDecision ? row.state === "closed" : ["ask", "review"].includes(row.state)))) return "read-only";
          if (local.bound !== row.revision && (local.draft.trim() || local.files?.length)) return "changed";
          return "actionable";
        },
        step: function(row, local, cap, ev, newId) {
          const st = status(row, local, cap), l5 = { ...local };
          const { start } = execution.module("answer-card.step", { start: (payload, revision = row.revision) => {
            l5.op = { id: newId(), task: row.n, revision, ...payload };
            if (payload.text != null && l5.files?.length) l5.op.files = JSON.parse(JSON.stringify(l5.files));
            l5.about = about(row, payload);
            l5.bound = row.revision;
            l5.receipt = { state: "sending" };
            return { local: l5, send: l5.op };
          } });
          switch (ev.type) {
            case "files":
              l5.files = ev.files;
              return { local: l5, send: null };
            case "input":
              if (st !== "changed") l5.bound = row.revision;
              l5.draft = ev.text;
              return { local: l5, send: null };
            case "choose": {
              if (!LIVE.includes(st) || l5.files?.length) return { local, send: null };
              const c4 = row.question.choices.find((x5) => x5.id === ev.id);
              if (!c4 || c4.consequential) return { local, send: null };
              if (row.resultDecision && c4.local) return { local: l5, send: null };
              if (row.resultDecision && c4.reason) return { local: { ...l5, intent: c4.reason, ...c4.reason === "later" && !l5.draft ? { draft: new Date(Date.now() + 7 * 864e5).toISOString().slice(0, 10) } : {} }, send: null };
              return start({ choice: c4.id });
            }
            case "submit":
              if (row.resultDecision && (!(row.question.resultReason || row.question.resultDate) || !l5.intent)) return { local, send: null };
              if (!cap || !cap.answer || !LIVE.includes(st) || textProblem(draftQuestion(row, cap), l5.draft, l5.files)) return { local, send: null };
              return start(fallback(row, cap) ? { action: "requeue", text: l5.draft.trim() } : { text: l5.draft.trim() }, st === "changed" && !fallback(row, cap) ? l5.bound : row.revision);
            case "cancel-result":
              return row.resultDecision && LIVE.includes(st) ? { local: { ...l5, intent: null }, send: null } : { local, send: null };
            case "requeue":
              if (!manageable(row, cap) || !LIVE.includes(st) || textProblem(REQUEUE, l5.draft, l5.files)) return { local, send: null };
              return start({ action: "requeue", text: l5.draft.trim() });
            case "later":
            case "close":
              if (!manageable(row, cap) || !LIVE.includes(st)) return { local, send: null };
              return start(ev.type === "later" ? { action: "later" } : { action: "close" });
            case "receipt":
              if (!l5.op || ev.opId !== l5.op.id) return { local, send: null };
              l5.receipt = { state: ["accepted", "rejected", "stale", "unknown", "failed", "partial"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason, next: ev.next, ...ev.note ? { note: true } : {} };
              if (ev.state === "accepted" && (l5.op.text || l5.op.files?.length)) {
                l5.draft = "";
                l5.files = [];
              }
              if (ev.state === "accepted" && l5.intent === "later") l5.intent = null;
              return { local: l5, send: null };
            case "retry":
              return RETRY.includes(st) ? { local: { ...l5, receipt: { state: "sending" } }, send: l5.op } : { local, send: null };
            default:
              return { local, send: null };
          }
        },
        render: function(board, row, local, cap) {
          const q4 = row.question || { text: row.card || "", parsed: false, choices: [], evidence: [] }, st = status(row, local, cap);
          const id = "qac-" + String(board).replace(/[^\w-]/g, "_") + "-" + row.n, live = LIVE.includes(st), own = !(cap && cap.composer);
          const what = local.op ? "#" + row.n + " " + (local.about || about(row, local.op)) : "";
          let h5 = '<section class="qac" data-qac-state="' + st + '" aria-labelledby="' + id + '-t">';
          h5 += '<p class="qac-head" id="' + id + '-t"><span class="qac-mark" aria-hidden="true">?</span> <a href="' + href(row.url) + '" target="_blank" rel="noopener">#' + esc(row.n) + "</a> " + esc(row.title) + "</p>";
          if (own && cap?.files && st !== "read-only") h5 += replyFiles.markup(local.files, !live);
          if (manageable(row, cap)) {
            h5 += cap?.paged ? images(q4) : q4.parsed ? lines(q4) + images(q4) : '<pre class="qac-text">' + esc(q4.text || row.card || "No question text supplied") + "</pre>";
            if (own) h5 += q4.parsed ? '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="2" aria-label="Your answer" placeholder="What you did"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(q4, local.draft, local.files) ? "" : " disabled") + ">Send</button>" + hint(id, live && textProblem(q4, local.draft, local.files)) + "</form>" : '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="1" aria-label="Requeue reason" placeholder="Reason to requeue"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(REQUEUE, local.draft, local.files) ? "" : " disabled") + ">Requeue</button>" + hint(id, live && textProblem(REQUEUE, local.draft, local.files)) + "</form>";
            h5 += '<div class="qac-choices">' + (q4.parsed && own ? '<button type="button" data-qac="requeue"' + (live && !textProblem(REQUEUE, local.draft, local.files) ? "" : " disabled") + ">Requeue</button>" : "") + '<button type="button" data-qac="later"' + (live ? "" : " disabled") + '>Later</button><button type="button" data-qac="close"' + (live ? "" : " disabled") + ">Close without publishing</button></div>";
            h5 += receipt(st, local.receipt && local.receipt.reason, row.url, id, local.receipt && local.receipt.next, !!(local.op && local.op.action), what, local.receipt?.note);
            h5 += retry(st, local.receipt && local.receipt.reason);
            return h5 + "</section>";
          }
          if (!q4.parsed) return h5 + (cap?.paged ? "" : '<pre class="qac-text">' + esc(q4.text) + "</pre>") + receipt(st, st === "read-only" ? "This card has no answer form here" : local.receipt && local.receipt.reason, row.url, id, null, false, what) + retry(st, local.receipt && local.receipt.reason) + "</section>";
          h5 += (cap?.paged ? "" : lines(q4)) + images(q4);
          if (q4.choices.length) h5 += '<div class="qac-choices" role="group" aria-label="Choices">';
          for (const c4 of q4.choices) {
            const off = !live || c4.consequential || !!local.files?.length, why = c4.consequential ? "Answer on the board: this choice closes or publishes the task, or approves its result" : local.files?.length ? "Send files with a text reply, or remove them to choose an option" : "";
            const name = c4.label == null ? c4.text : c4.label, said = [c4.detail ? id + "-o" + c4.id : "", why ? id + "-c" + c4.id : ""].filter(Boolean).join(" ");
            h5 += '<button type="button" data-qac="choose" data-choice="' + c4.id + '"' + (off ? " disabled" : "") + (said ? ' aria-describedby="' + said + '"' : "") + '><span class="qac-number">' + c4.id + ".</span> <span>" + esc(label(name)) + (c4.recommended ? ' <span class="qac-star" role="img" aria-label="Recommended">\u2605</span>' : "") + "</span></button>";
            for (const m5 of String(name).matchAll(LINK)) h5 += '<a class="qac-link" href="' + href(m5[2]) + '" target="_blank" rel="noopener">' + esc(m5[1]) + " \u2197</a>";
            if (c4.detail) h5 += '<span class="qac-detail" id="' + id + "-o" + c4.id + '">' + linked(c4.detail) + "</span>";
            if (why) h5 += '<span class="qac-why" id="' + id + "-c" + c4.id + '">' + esc(why) + ' <a href="' + href(row.url) + '" target="_blank" rel="noopener">board \u2197</a></span>';
          }
          if (q4.choices.length) h5 += "</div>";
          if (row.pick) h5 += '<p class="qac-pick"><span class="qac-muted">PM</span> ' + esc(row.n) + "." + esc(row.pick.id) + ": " + esc(row.pick.reason) + "</p>";
          if (own && (st !== "read-only" || local.draft)) {
            const bad = textProblem(q4, local.draft, local.files);
            h5 += '<details class="qac-other"' + (local.draft || local.files?.length || !q4.choices.length ? " open" : "") + "><summary>" + (q4.choices.length ? "Other answer" : "Answer") + '</summary><form class="qac-text-form" data-qac="submit"><textarea id="' + id + '-d" data-qac="input" rows="2" aria-label="Your answer" placeholder="' + (q4.choices.length ? "Or type an answer" : "What you did") + '"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !bad ? "" : " disabled") + ">Send</button>" + hint(id, live && bad) + "</form></details>";
          }
          const reason = st === "read-only" ? cap && cap.reason || "Answering is not available here" : local.receipt && local.receipt.reason;
          h5 += receipt(st, reason, row.url, id, local.receipt && local.receipt.next, false, what, local.receipt?.note);
          h5 += retry(st, local.receipt && local.receipt.reason);
          const links = q4.evidence.filter((e4) => !e4.image);
          if (links.length) h5 += '<details class="qac-context"><summary>Evidence</summary><p class="qac-evidence">' + links.map((e4) => '<a href="' + href(e4.url) + '" target="_blank" rel="noopener">' + esc(e4.label) + "</a>").join(" \xB7 ") + "</p></details>";
          return h5 + "</section>";
        },
        compose: function(row, local, cap) {
          const q4 = row.question || {}, st = status(row, local, cap), text = !!(cap && cap.answer) && (fallback(row, cap) || !!q4.parsed);
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
            choices: !!(q4.choices && q4.choices.length),
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
              return [...cards].map(([k5, c4]) => '<div data-qac-key="' + esc(k5) + '">' + render(c4.board, c4.row, locals.get(k5), adapter.capability(c4.board, c4.row)) + "</div>").join("");
            },
            apply: function(k5, ev) {
              const { board, row } = cards.get(k5), cap = adapter.capability(board, row), r4 = step(row, locals.get(k5), cap, ev, newId);
              locals.set(k5, r4.local);
              adapter.save?.(board, row, r4.local);
              if (ev.type !== "input") paint();
              if (r4.send) ui.effect("answer.send", () => adapter.send(board, r4.send), (x5) => {
                if (cards.has(k5)) apply(k5, { type: "receipt", opId: r4.send.id, ...x5 });
              });
              else if (ev.type === "input") {
                const s4 = el.querySelector('[data-qac-key="' + CSS.escape(k5) + '"] button[type=submit]'), cap2 = adapter.capability(board, row);
                const live = LIVE.includes(status(row, r4.local, cap2)), bad = textProblem(draftQuestion(row, cap2), r4.local.draft, r4.local.files);
                if (s4) s4.disabled = !!bad || !live;
                const rq = el.querySelector('[data-qac-key="' + CSS.escape(k5) + '"] [data-qac=requeue]');
                if (rq) rq.disabled = !live || !!textProblem(REQUEUE, r4.local.draft, r4.local.files);
                const h5 = s4 && s4.parentNode.querySelector(".qac-hint");
                if (h5) h5.textContent = live && bad || "";
                painted = markup();
              }
            },
            paint: function() {
              const a4 = document.activeElement, focus = a4 && el.contains(a4) ? { k: a4.closest("[data-qac-key]")?.dataset.qacKey, sel: a4.dataset.qac === "input" ? "textarea" : a4.dataset.choice ? '[data-choice="' + a4.dataset.choice + '"]' : '[data-qac="' + a4.dataset.qac + '"]', s: a4.selectionStart, e: a4.selectionEnd } : null;
              const scroll = el.scrollTop, open = new Set([...el.querySelectorAll("details[open]")].map((d5) => d5.closest("[data-qac-key]")?.dataset.qacKey + " " + d5.className));
              const html = markup();
              if (html === painted) return;
              el.innerHTML = painted = html;
              for (const d5 of el.querySelectorAll("details")) if (open.has(d5.closest("[data-qac-key]").dataset.qacKey + " " + d5.className)) d5.open = true;
              el.scrollTop = scroll;
              for (const [k5, local] of locals) if (Array.isArray(local.caret) && local.caret.length === 2) {
                const input = el.querySelector('[data-qac-key="' + CSS.escape(k5) + '"] textarea');
                input?.setSelectionRange(...local.caret);
              }
              const t4 = focus && focus.k && el.querySelector('[data-qac-key="' + CSS.escape(focus.k) + '"] ' + focus.sel);
              if (t4) {
                t4.focus();
                if (focus.s != null && t4.setSelectionRange) t4.setSelectionRange(focus.s, focus.e);
              }
            }
          });
          const locals = /* @__PURE__ */ new Map(), cards = /* @__PURE__ */ new Map();
          let painted = null;
          const newId = adapter.newId || (() => root.crypto && root.crypto.randomUUID ? root.crypto.randomUUID() : "qac-" + Date.now().toString(36) + Math.random().toString(36).slice(2));
          ui.listen(el, "click", (e4) => {
            const b5 = e4.target.closest("button[data-qac]"), k5 = b5 && b5.closest("[data-qac-key]")?.dataset.qacKey;
            if (k5 && !b5.disabled) apply(k5, b5.dataset.qac === "choose" ? { type: "choose", id: Number(b5.dataset.choice) } : b5.dataset.qac === "remove-file" ? { type: "files", files: (locals.get(k5).files || []).filter((f4) => f4.id !== b5.dataset.file) } : { type: b5.dataset.qac });
          });
          const add = (k5, files) => {
            if (!k5 || !adapter.capability(cards.get(k5).board, cards.get(k5).row).files || !LIVE.includes(status(cards.get(k5).row, locals.get(k5), adapter.capability(cards.get(k5).board, cards.get(k5).row)))) return;
            replyFiles.read(files, (file) => {
              if (cards.has(k5) && (file.pending || locals.get(k5).files?.some((f4) => f4.id === file.id))) apply(k5, { type: "files", files: [...(locals.get(k5).files || []).filter((f4) => f4.id !== file.id), file] });
            });
          };
          ui.listen(el, "change", (e4) => {
            if (e4.target.dataset.qac === "files") {
              add(e4.target.closest("[data-qac-key]")?.dataset.qacKey, [...e4.target.files]);
            }
          });
          ui.listen(el, "paste", (e4) => {
            if (e4.clipboardData?.files.length) {
              e4.preventDefault();
              add(e4.target.closest("[data-qac-key]")?.dataset.qacKey, [...e4.clipboardData.files]);
            }
          });
          ui.listen(el, "dragover", (e4) => e4.preventDefault());
          ui.listen(el, "drop", (e4) => {
            if (e4.dataTransfer?.files.length) {
              e4.preventDefault();
              add(e4.target.closest("[data-qac-key]")?.dataset.qacKey, [...e4.dataTransfer.files]);
            }
          });
          ui.listen(el, "submit", (e4) => {
            e4.preventDefault();
            const k5 = e4.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k5) apply(k5, { type: "submit" });
          });
          ui.listen(el, "keydown", (e4) => {
            if (e4.target.dataset.qac === "input" && e4.key === "Enter" && !e4.shiftKey && !e4.isComposing && e4.keyCode !== 229) {
              e4.preventDefault();
              const k5 = e4.target.closest("[data-qac-key]")?.dataset.qacKey;
              if (k5) apply(k5, { type: "submit" });
            }
          });
          for (const event of ["select", "keyup", "click"]) ui.listen(el, event, (e4) => {
            const k5 = e4.target.dataset.qac === "input" && e4.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k5 && adapter.save) {
              const c4 = cards.get(k5), local = { ...locals.get(k5), caret: [e4.target.selectionStart, e4.target.selectionEnd] };
              locals.set(k5, local);
              adapter.save(c4.board, c4.row, local);
            }
          });
          ui.listen(el, "input", (e4) => {
            const k5 = e4.target.dataset.qac === "input" && e4.target.closest("[data-qac-key]")?.dataset.qacKey;
            if (k5) apply(k5, { type: "input", text: e4.target.value });
          });
          return execution.module("answer-card.mount.controller", {
            files(board, n3, files) {
              const k5 = key(board, { n: n3 });
              if (!cards.has(k5)) return false;
              apply(k5, { type: "files", files });
              return true;
            },
            receiveReceipt(board, opId, reply) {
              for (const [k5, card] of cards) if (card.board === board && locals.get(k5)?.op?.id === opId) {
                apply(k5, { ...reply, type: "receipt", opId });
              }
            },
            // #1662 A3: an adapter whose capability has `composer` owns text entry in its shared composer and drives the card here;
            // text null records only the caret
            input(board, n3, text, caret) {
              const k5 = key(board, { n: n3 });
              if (!cards.has(k5)) return;
              if (text != null && text !== locals.get(k5).draft) apply(k5, { type: "input", text });
              if (caret) {
                const c4 = cards.get(k5), local = { ...locals.get(k5), caret };
                locals.set(k5, local);
                adapter.save?.(c4.board, c4.row, local);
              }
            },
            act(board, n3, type) {
              const k5 = key(board, { n: n3 });
              if (cards.has(k5) && ["submit", "requeue", "cancel-result"].includes(type)) apply(k5, { type });
            },
            // rows: [{board, row}] of open questions; a row that left keeps no state
            update(rows) {
              cards.clear();
              for (const c4 of rows) {
                const k5 = key(c4.board, c4.row);
                cards.set(k5, c4);
                if (!locals.has(k5)) locals.set(k5, adapter.load?.(c4.board, c4.row) || fresh(c4.row));
              }
              for (const k5 of [...locals.keys()]) if (!cards.has(k5)) locals.delete(k5);
              paint();
            }
          });
        },
        replay: function(fx, capability) {
          let row = fx.rows[0], local = fresh(row), n3 = 0;
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
            const r4 = step(row, local, capability(fx.board, row), ev.type === "receipt" ? { ...ev, opId: sent[sent.length - 1] && sent[sent.length - 1].id } : ev, () => "op-" + ++n3);
            local = r4.local;
            if (r4.send) sent.push(r4.send);
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
      var { esc } = execution.module("answer-card", { esc: (s4) => String(s4 == null ? "" : s4).replace(/[&<>"']/g, (c4) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c4]) });
      var { key } = execution.module("answer-card", { key: (board, row) => board + "#" + row.n });
      var { href } = execution.module("answer-card", { href: (u4) => /^https?:\/\//i.test(u4 || "") ? esc(u4) : "#" });
      var LINK = /\[([^\]]*)\]\(([^)\s]*)\)/g;
      var { label } = execution.module("answer-card", { label: (s4) => String(s4 == null ? "" : s4).replace(LINK, "$1") });
      var { linked } = execution.module("answer-card", { linked: (s4) => {
        const p5 = String(s4 == null ? "" : s4).split(LINK);
        let h5 = esc(p5[0]);
        for (let i4 = 1; i4 < p5.length; i4 += 3) h5 += '<a href="' + href(p5[i4 + 1]) + '" target="_blank" rel="noopener">' + esc(p5[i4]) + "</a>" + esc(p5[i4 + 2]);
        return h5;
      } });
      var { images } = execution.module("answer-card", { images: (q4) => {
        const im = (q4.evidence || []).filter((e4) => e4.image), n3 = (q4.choices || []).length, each = n3 >= 2 && im.length === n3;
        return im.length ? '<p class="qac-images">' + im.map((e4, i4) => (each ? '<span class="qac-shot">' : "") + '<a href="' + href(e4.url) + '" target="_blank" rel="noopener" title="Open ' + esc(e4.label) + ' full size">' + (/^data:image\/(png|jpeg|gif|webp);base64,/.test(e4.src || "") ? '<img class="qac-thumb" src="' + esc(e4.src) + '" alt="' + esc(e4.label) + '">' : esc(e4.label) + " \u2197") + "</a>" + (each ? '<span class="qac-caption">' + (i4 + 1) + ".</span></span>" : "")).join("") + "</p>" : "";
      } });
      var { lines } = execution.module("answer-card", { lines: (q4) => (q4.done ? '<p class="qac-done"><span class="qac-icon" role="img" aria-label="Done">\u2713</span> ' + linked(q4.done) + "</p>" : "") + '<p class="qac-problem"><span class="qac-icon" role="img" aria-label="Problem">\u2691</span> ' + linked(q4.problem) + "</p>" });
      var { manageable } = execution.module("answer-card", { manageable: (row, cap) => row.state === "ask" && cap && cap.answer && cap.manage && !(row.question && row.question.choices && row.question.choices.length) });
      var { fallback } = execution.module("answer-card", { fallback: (row, cap) => manageable(row, cap) && !(row.question && row.question.parsed) });
      var { about } = execution.module("answer-card", { about: (row, op) => {
        if (op.action) return { requeue: "requeue", later: "later", close: "close without publication" }[op.action] || op.action;
        if (op.text != null) return "text answer";
        const c4 = row.question && row.question.choices && row.question.choices.find((x5) => x5.id === op.choice);
        return "answer " + op.choice + (c4 ? ". " + label(c4.label == null ? c4.text : c4.label) : "");
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

  // src/pm-composer.js
  var require_pm_composer = __commonJS({
    "src/pm-composer.js"(exports, module) {
      "use strict";
      var { h: h5, render } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      var measured = /* @__PURE__ */ new WeakMap();
      var api = execution.module("pm-composer", {
        request: function(context, text, op, now) {
          if (!text.trim() || new TextEncoder().encode(text).length > 65536) throw Error("Write 1\u201365536 UTF-8 bytes");
          const target = Object.fromEntries(["installation_id", "generation", "project_id", "runtime_id"].map((k5) => [k5, context.target?.[k5]]));
          const request = { schema: "taskq.pm/1", actor_id: context.actor_id, op_id: op, grant_id: context.grant_id, revision: context.revision, target, issued_at: now, expires_at: now + 300, text };
          for (const id of [request.actor_id, op, request.grant_id, request.revision, context.target?.installation_id, context.target?.project_id, context.target?.runtime_id]) if (!/^[A-Za-z0-9_-]{1,64}$/.test(id || "")) throw Error("PM target or authority unavailable");
          if (!Number.isSafeInteger(context.target.generation) || context.target.generation < 1 || !Number.isSafeInteger(now) || now < 0) throw Error("PM identity or clock unavailable");
          return JSON.parse(JSON.stringify(request));
        },
        receipt: function(value, request) {
          if (!value || value.schema !== "taskq.pm/1" || !["accepted", "rejected", "stale", "unknown"].includes(value.outcome) || ["actor_id", "op_id", "revision"].some((k5) => value[k5] !== request[k5])) return { outcome: "unknown", reason: "No matching authoritative receipt" };
          if (value.outcome === "accepted" && (!/^[a-f0-9]{64}$/.test(value.request_id || "") || !value.native?.host || !value.native?.runtime)) return { outcome: "unknown", reason: "Native request identity unavailable" };
          return JSON.parse(JSON.stringify(value));
        },
        handoff: function(native, local = false) {
          if (!native) return null;
          const url = native.url, safe = typeof url === "string" && (/^https:\/\/claude\.ai\/code\/session_[A-Za-z0-9]+$/.test(url) || local && /^codex:\/\/threads\/[A-Za-z0-9-]+$/.test(url));
          return h5("div", { class: "pm-handoff" }, h5("p", { role: "status" }, (native.kind || native.state || "unknown") + ": " + (native.reason || native.next || (native.state === "launched" && native.session ? "Continue in the native session" : "Native outcome unknown"))), h5("p", { class: "meta" }, (native.runtime || "Runtime unknown") + " \xB7 " + (native.host || "Host unknown") + (native.session ? " \xB7 " + native.session : "")), safe && h5("a", { href: url, "data-url": url, class: "native-session" }, "Open session"));
        },
        grow: function(root) {
          const window2 = root.ownerDocument.defaultView;
          for (const input of root.querySelectorAll(".conversation-composer textarea")) {
            const style = window2.getComputedStyle(input), width = input.getBoundingClientRect().width, key = JSON.stringify([input.value, width, style.font, style.lineHeight, style.minHeight]);
            if (measured.get(input) === key) continue;
            const mirror = input.cloneNode();
            mirror.removeAttribute("id");
            mirror.removeAttribute("data-c-field");
            mirror.disabled = true;
            mirror.tabIndex = -1;
            mirror.setAttribute("aria-hidden", "true");
            mirror.value = input.value;
            Object.assign(mirror.style, { position: "absolute", visibility: "hidden", pointerEvents: "none", width: width + "px", height: "0", minHeight: "0", maxHeight: "none" });
            input.after(mirror);
            const height = mirror.scrollHeight + parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth);
            mirror.remove();
            input.style.height = Math.max(parseFloat(style.minHeight) || 0, height) + "px";
            measured.set(input, key);
          }
        },
        composer: function({ text, disabled = false, id = "pm-draft", field = "text", label = "Request draft", reason = "", placeholder = "Ask AI\u2026", before, files, tools, after, onInput, onPaste, onDrop, onDragOver }) {
          return h5("div", { class: "conversation-composer", onPaste, onDrop, onDragOver }, before, h5("label", { for: id, class: "sr-only" }, label), h5("textarea", { id, "data-c-field": field, rows: 3, value: text, disabled, "aria-describedby": id === "conversation-draft" ? "conversation-reason" : id + "-reason", placeholder, onInput }), files, tools, h5("p", { id: id === "conversation-draft" ? "conversation-reason" : id + "-reason", class: "meta", title: reason, "data-tooltip": true, tabIndex: reason ? 0 : void 0 }, reason), after);
        },
        mount: function(el, adapter) {
          let draft = adapter.load() || { text: "" }, pending = false, timer, serial = 0, invalid = false;
          if (draft.request) {
            try {
              const r4 = draft.request, expected = api.request(r4, r4.text, r4.op_id, r4.issued_at);
              if (r4.schema !== expected.schema || r4.expires_at !== expected.expires_at) throw Error("Invalid saved request");
              draft.request = expected;
              if (draft.receipt) draft.receipt = api.receipt(draft.receipt, expected);
            } catch (_5) {
              invalid = true;
              draft.receipt = { outcome: "unknown", reason: "Saved request invalid; preserve it for host reconciliation" };
            }
          } else if (draft.receipt?.native) draft.receipt = { outcome: "unknown", reason: "No bound request for this receipt" };
          const local = execution.module("pm-composer.instance", {
            paint: function() {
              render(h5("section", { class: "pm-composer", "aria-label": "PM request" }, api.composer({ text: draft.text, disabled: !!draft.request, reason: pending ? "Checking delivery\u2026" : draft.receipt?.reason || adapter.context().reason || "Replies stay in the native session.", onInput: (e4) => {
                draft.text = e4.currentTarget.value;
                adapter.save(draft);
                local.paint();
              }, tools: h5("div", { class: "conversation-tools" }, h5("button", { disabled: pending || invalid || !!draft.request || !draft.text.trim() || !!adapter.context().reason, onClick: () => local.send(false) }, "Send"), draft.request && h5("button", { disabled: pending || invalid || !adapter.reconcile, onClick: () => local.send(true) }, "Check delivery")) }), draft.receipt && h5("p", { role: "status" }, draft.receipt.outcome), api.handoff(draft.receipt?.native)), el);
              api.grow(el);
            },
            send: function(reconcile) {
              if (invalid || pending || reconcile && !draft.request || !reconcile && draft.request) return;
              if (!reconcile) {
                try {
                  const next = { ...draft, request: api.request(adapter.context(), draft.text, adapter.id(), adapter.now()) };
                  adapter.save(next);
                  draft = next;
                } catch (_5) {
                  draft.receipt = { outcome: "unknown", reason: "Request could not be persisted; draft kept" };
                  local.paint();
                  return;
                }
              }
              pending = true;
              const turn = ++serial;
              local.paint();
              ui.cancel(timer);
              timer = ui.defer(() => {
                if (turn !== serial) return;
                pending = false;
                draft.receipt = { outcome: "unknown", reason: "Delivery unknown. Check the same request ID." };
                adapter.save(draft);
                local.paint();
              }, 3e4);
              ui.effect("pm.dispatch", () => (reconcile ? adapter.reconcile : adapter.send)(JSON.parse(JSON.stringify(draft.request))), (value) => {
                if (turn !== serial) return;
                ui.cancel(timer);
                draft.receipt = api.receipt(value, draft.request);
                pending = false;
                adapter.save(draft);
                local.paint();
              });
            }
          });
          local.paint();
          return local;
        }
      });
      module.exports = api;
    }
  });

  // src/anchored-popover.js
  var require_anchored_popover = __commonJS({
    "src/anchored-popover.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var { execution } = require_ui_runtime();
      module.exports = execution.module("anchoredPopover", {
        render: function({ id = "taskq-popover", label, children }) {
          return h5("section", { id, popover: "manual", role: "dialog", "aria-label": label, class: "anchored-popover", tabindex: -1 }, children);
        },
        place: function(element, anchor) {
          if (!element || !anchor) return;
          if (element.showPopover && !element.matches(":popover-open")) element.showPopover();
          const gap = 6, pad = 8, w5 = window.innerWidth, h6 = window.innerHeight;
          element.style.maxWidth = Math.max(0, w5 - 2 * pad) + "px";
          element.style.maxHeight = Math.max(0, h6 - 2 * pad) + "px";
          const r4 = element.getBoundingClientRect(), below = h6 - anchor.bottom - gap - pad, above = anchor.top - gap - pad;
          const top = below >= r4.height || below >= above ? anchor.bottom + gap : anchor.top - gap - r4.height;
          element.style.left = Math.max(pad, Math.min(anchor.left, w5 - r4.width - pad)) + "px";
          element.style.top = Math.max(pad, Math.min(top, h6 - r4.height - pad)) + "px";
        }
      });
    }
  });

  // src/task-dashboard.js
  var require_task_dashboard = __commonJS({
    "src/task-dashboard.js"(exports, module) {
      "use strict";
      var { h: h5, Fragment } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      var views = require_task_views();
      var blocks = require_project_blocks();
      var popover = require_anchored_popover();
      var { icon, button } = require_ui_icons2();
      var outcomes = { success: ["circle-check", "", "Completed"], not_planned: ["circle-slash", "", "Not planned"], unknown: ["circle-question-mark", "", "Outcome unknown"] };
      module.exports = execution.module("taskDashboard", {
        outcome: function(t4) {
          return t4.outcome === "success" && typeof t4.merged === "string" && t4.merged ? ["git-merge", "review", "Merged"] : outcomes[t4.outcome] || outcomes.unknown;
        },
        // P-W4: `owned` is the View window surface; the menu sends `views-open` when native offers that window, else keeps the overlay.
        create: function(view, { menu, present, send, app, owned = false, window: window2 = globalThis, document: document2 = window2.document }) {
          const crypto2 = window2.crypto || globalThis.crypto, CSS2 = window2.CSS || globalThis.CSS;
          let restore = true, requests = 0, heldOffer = null, heldReview = null;
          const pinReads = /* @__PURE__ */ new Map();
          const api = execution.module("taskDashboard.instance", {
            model: function() {
              const m5 = view.dashboard || (view.dashboard = views.normalize(null, { taskFilter: view.taskFilter }));
              if (!owned && m5.selected === "completed") {
                const c4 = m5.views.find((v4) => v4.id === "completed")?.working;
                m5.selected = m5.views.find((v4) => v4.id === "running")?.id || "all";
                send({ a: "workspace", open: { page: "conversations", filter: "finished", criteria: c4 } });
              }
              m5.collapsed = m5.collapsed.filter((id) => view.S.projects.some((p5) => views.identity(p5) === id));
              return m5;
            },
            current: function() {
              const m5 = api.model();
              return owned && view.completedEditor ? m5.views.find((v4) => v4.id === "completed") || views.current(m5) : views.current(m5);
            },
            criteria: function() {
              return api.current().working;
            },
            set: function(patch) {
              Object.assign(api.criteria(), patch);
              present();
            },
            projects: function() {
              return views.scope(view.S.projects, api.criteria());
            },
            unique: function(ps) {
              return ps.filter((p5, i4) => ps.findIndex((q4) => views.identity(q4) === views.identity(p5)) === i4);
            },
            live: function() {
              return app.unique(view.S.projects.flatMap(app.tasks)).filter((t4) => t4.state !== "closed");
            },
            ref: function(t4) {
              return t4.boardIdentity + "#" + t4.n;
            },
            rows: function() {
              return app.unique(api.projects().flatMap(app.tasks)).filter((t4) => t4.state !== "closed" && views.matches(t4, api.criteria().query)).sort(views.compare);
            },
            heldScope: function() {
              const c4 = api.criteria(), ps = api.projects(), targets = api.rows().filter((t4) => t4.state === "held").map((t4) => ({ key: t4.key, revision: t4.revision, label: t4.project + " #" + t4.n + " " + t4.title }));
              const known = ps.length > 0 && ps.every((p5) => p5.alive === true && p5.complete === true && p5.tasks_live !== false && !p5.partial?.length && !view.S.display.all.partial.includes(p5.name)) && c4.projects.every((id) => ps.some((p5) => views.identity(p5) === id)) && targets.every((t4) => Number.isSafeInteger(t4.revision));
              const scope = ps.map((p5) => p5.name).join(", ") + " \xB7 view " + api.current().name + " \xB7 " + (c4.query ? "search \u201C" + c4.query + "\u201D" : "all searches");
              return { targets, known, scope, signature: JSON.stringify([api.current().id, c4.projects, c4.query, targets]) };
            },
            admissionReview: function() {
              return heldReview && h5("section", { class: "held-review", role: "region", "aria-label": "Review Held admission" }, h5("strong", null, "Start " + heldReview.targets.length + " held"), h5("p", null, heldReview.scope), h5("p", { class: "meta" }, "Admit only these drafts. Existing focus, search and view stay unchanged."), h5("ul", null, heldReview.targets.map((t4) => h5("li", { key: t4.key }, h5("a", { href: t4.key, "data-url": t4.key }, t4.label), h5("span", { class: "meta" }, " \xB7 revision " + t4.revision)))), heldReview.error && h5("p", { role: "alert" }, heldReview.error), heldReview.sent && h5("p", { role: "status" }, "Start requested once; check each receipt below."), h5("div", { class: "row-actions" }, h5("button", { "data-a": "held-confirm", disabled: !!heldReview.sent }, "Confirm Start " + heldReview.targets.length + " held"), h5("button", { "data-a": "held-cancel" }, heldReview.sent ? "Close Held admission" : "Cancel Held admission")));
            },
            anchor: function(target) {
              const r4 = target.getBoundingClientRect();
              return { left: r4.left, top: r4.top, bottom: r4.bottom };
            },
            // MP13: details open only from click, Enter or Space on the identity (native button activation); `focus` moves focus into them.
            open: function(kind, target, key = null, focus = true) {
              view.popup = { kind, key, anchor: api.anchor(target) };
              if (kind === "task") api.current().open = key;
              present();
              if (focus) menu.querySelector("#taskq-question-details,#taskq-popover")?.focus({ preventScroll: true });
            },
            close: function(focus = true) {
              const p5 = view.popup;
              if (!p5) return;
              view.popup = null;
              if (p5.kind === "task") api.current().open = null;
              present();
              if (focus) {
                const target = p5.kind === "task" ? menu.querySelector('[data-a="task-detail"][data-key="' + CSS2.escape(p5.key) + '"]') : menu.querySelector('[data-a="' + (p5.kind === "views" ? "view-list" : "view-editor") + '"]');
                (target || menu.querySelector("[data-a=view-list]"))?.focus({ preventScroll: true });
              }
            },
            // M2: collapse is presentation only; the name and the two adjacent shortcuts keep independent targets.
            projectHeader: function(p5, shown, tasks, reason) {
              const board = p5.board_kind === "gitlab" ? "GitLab" : "GitHub", id = views.identity(p5), open = !api.model().collapsed.includes(id), controls = "project-tasks-" + encodeURIComponent(id), folder = typeof p5.root === "string" && !!p5.root.trim();
              const questions = tasks.filter((t4) => t4.state === "ask").length, label = reason ? "Matching and total task counts and open questions unknown: " + reason : shown + " tasks shown matching the current filter; " + tasks.length + " open tasks in total; " + questions + " open questions";
              const statuses = !open && h5("div", { class: "project-statuses" }, reason ? h5("span", { role: "img", title: "Status counts unknown: " + reason, "aria-label": "Status counts unknown: " + reason }, "?") : view.S.display.states.map(({ id: state, name }) => {
                const n3 = tasks.filter((t4) => t4.state === state).length;
                if (!n3) return null;
                const [mark, tone] = blocks.marks[state] || ["circle-question-mark", ""];
                return h5("span", { class: "project-state " + tone, "data-state": state, role: "img", title: name + ": " + n3, "aria-label": name + ": " + n3, key: state }, icon(mark), n3);
              }));
              return h5(
                "header",
                { class: "project-group project-shortcuts" },
                button("arrow-down", (open ? "Collapse " : "Expand ") + p5.name + " tasks", { "data-a": "project-collapse", "data-project": id, "aria-expanded": String(open), "aria-controls": controls, class: "compact-action project-collapse" }),
                h5("div", { class: "project-heading" }, h5("h2", null, h5("button", { class: "project-overview", "data-route": p5.root, title: "Open " + p5.name + " project overview" }, p5.name)), h5("span", { class: "project-counts", title: label, "aria-label": label }, h5("span", null, (reason ? "?" : shown) + " / " + (reason ? "?" : tasks.length)), (reason || questions > 0) && h5("span", { class: "project-questions" }, " (?" + (reason ? "?" : questions) + ")")), statuses),
                h5("div", { class: "project-actions" }, h5("a", { class: "project-board compact-action", href: p5.issues, "data-url": p5.issues, title: "Open " + p5.name + " board on " + board, "aria-label": "Open " + p5.name + " board on " + board }, icon(p5.board_kind)), button("folder", folder ? "Open " + p5.name + " project folder on this computer" : "Project folder unavailable: registered root missing", { "data-a": "folder", "data-root": p5.root, disabled: !folder, class: "compact-action project-folder" }))
              );
            },
            bar: function() {
              const m5 = api.model(), v4 = api.current(), c4 = v4.working, searching = !!c4.query || !!view.searchOpen;
              return h5(
                Fragment,
                null,
                h5("div", { class: "viewbar" }, h5("div", { class: "view-picks" }, h5("button", { "data-a": "view-list", "aria-haspopup": "dialog", "aria-expanded": String(view.popup?.kind === "views") }, v4.name, views.dirty(v4) ? " *" : ""), m5.views.filter((x5) => x5.pinned && x5.id !== m5.selected).map((x5) => h5("button", { "data-a": "view-select", "data-id": x5.id, key: x5.id }, x5.name))), views.dirty(v4) && button("rotate-ccw", "Reset " + v4.name + " to its saved criteria", { "data-a": "view-reset" }), button("search", "Search tasks", { "data-a": "search-open", "aria-controls": "task-query", "aria-expanded": String(searching) }), button("sliders-horizontal", "Edit view conditions", { "data-a": "view-editor", "aria-haspopup": "dialog", "aria-expanded": String(view.popup?.kind === "editor") })),
                // #1775: search is one icon until opened; a query keeps the field visible
                searching && h5("div", { class: "task-search" }, icon("search"), h5("input", { id: "task-query", type: "search", "data-task-query": "", maxLength: 1024, "aria-label": "Search tasks", placeholder: "Search tasks", value: c4.query }), c4.query && button("x", "Clear task search", { "data-a": "search-clear" }))
              );
            },
            // P-W1: global pins once, first, as ordinary rows in the one shared scroll; unavailable references keep a sourced outcome or explicit unknown.
            // Exact reads use canonical board identity, never a pin URL or the loaded Completed page.
            pinRead: function(pin, retry = false) {
              let e4 = pinReads.get(pin.ref);
              const now = Date.now();
              if (e4?.loading || e4 && !retry && now - e4.at < 6e4) return e4;
              const cut = pin.ref.lastIndexOf("#"), n3 = Number(pin.ref.slice(cut + 1)), p5 = view.S.projects.find((p6) => views.identity(p6) === pin.ref.slice(0, cut));
              if (!p5 || !Number.isSafeInteger(n3) || n3 <= 0) return { reason: "Registered board unavailable" };
              e4 = { ...e4, ref: pin.ref, root: p5.root, task: n3, kind: p5.board_kind, project: p5.name, id: "pin-" + ++requests, at: now, loading: true, reason: null };
              pinReads.set(pin.ref, e4);
              e4.timer = ui.defer(() => api.receive({ root: e4.root, id: e4.id, reason: "No board response within 65 seconds" }), 65e3);
              send({ a: "completed", root: e4.root, id: e4.id, task: n3 });
              return e4;
            },
            unpin: function(ref) {
              ui.cancel(pinReads.get(ref)?.expiry);
              api.model().pins = api.model().pins.filter((p5) => p5.ref !== ref);
            },
            pins: function() {
              const m5 = api.model();
              if (!m5.pins.length) return null;
              const live = api.live();
              const rows = m5.pins.map((pin) => {
                const t4 = live.find((t5) => api.ref(t5) === pin.ref);
                if (t4) {
                  delete pin.closedSeen;
                  const prior = pinReads.get(pin.ref);
                  ui.cancel(prior?.timer);
                  ui.cancel(prior?.expiry);
                  pinReads.delete(pin.ref);
                  return app.taskRow(t4, true);
                }
                const e4 = api.pinRead(pin), c4 = e4?.value, closed = c4?.state === "closed";
                if (closed) {
                  pin.closedSeen = Number.isFinite(c4.closed) && c4.closed > 0 ? c4.closed : pin.closedSeen || view.S.now;
                  const remaining = pin.closedSeen + 86400 - view.S.now;
                  if (!e4.expiry && remaining > 0) e4.expiry = ui.defer(() => {
                    const current = pinReads.get(pin.ref);
                    if (current) current.expiry = null;
                    if (current?.value?.state === "closed" && !api.live().some((t5) => api.ref(t5) === pin.ref)) {
                      api.unpin(pin.ref);
                      present();
                    }
                  }, remaining * 1e3);
                  if (Number.isFinite(remaining) && remaining <= 0) {
                    api.unpin(pin.ref);
                    return null;
                  }
                }
                const [mark, tone, label] = closed ? api.outcome(c4) : ["circle-question-mark", "", c4?.state === "open" ? "Open \xB7 refresh project" : e4?.loading ? "Reading board\u2026" : "Unavailable \xB7 state unknown"];
                return h5(
                  "div",
                  { class: "task-row completed-row", key: pin.ref, "data-pin": pin.ref },
                  h5("span", { class: tone, title: label, "aria-label": label }, icon(mark)),
                  h5("a", { class: "detail-title", href: c4?.url || pin.url, "data-url": c4?.url || pin.url, "data-pin-open": pin.ref }, c4?.title || pin.title || pin.ref),
                  h5("div", { class: "meta" }, e4.project ? e4.project + " #" + e4.task + " \xB7 " : "", label, closed && c4.close ? " \xB7 " + c4.close : "", e4?.reason ? " \xB7 " + e4.reason : "", closed && c4.error ? " \xB7 Details unavailable: " + c4.error : "", e4?.reason && h5("button", { "data-a": "pin-retry", "data-ref": pin.ref }, "Retry board read")),
                  h5("div", { class: "row-actions" }, closed && h5(Fragment, null, h5("a", { href: c4.url, "data-url": c4.url, "data-pin-open": pin.ref }, "Open issue"), c4.pr && h5("a", { href: c4.pr, "data-url": c4.pr, "data-pin-open": pin.ref }, "Open PR"), h5("button", { "data-a": "pin-transcript", "data-ref": pin.ref }, "Open transcript")), button("pin-off", "Unpin " + (c4?.title || pin.title || pin.ref), { "data-a": "task-unpin", "data-ref": pin.ref }))
                );
              });
              return rows.some(Boolean) && h5("section", { class: "task-pins", "aria-label": "Pinned tasks" }, h5("h2", null, icon("pin"), " Pinned"), rows);
            },
            openCompleted: function(task) {
              const p5 = task && view.S.projects.find((p6) => p6.root === task.root);
              send({ a: "workspace", open: task ? { page: "conversations", root: task.root, board: p5?.issues, task: task.task, ...task.value?.sha ? { sha: task.value.sha } : {} } : { page: "conversations", filter: "finished", criteria: api.criteria() } });
            },
            list: function() {
              if (api.current().id === "completed") return h5(Fragment, null, api.pins(), api.bar(), h5("button", { "data-a": "completed-open" }, "Open Completed in Conversations"));
              const ps = api.projects(), c4 = api.criteria(), all = api.rows(), pinned = new Set(api.model().pins.map((p5) => p5.ref)), matching = all.filter((t4) => !c4.states.length || c4.states.includes(t4.state)), rows = matching.filter((t4) => !pinned.has(api.ref(t4))), live = api.live();
              const loaded = ps.filter((p5) => p5.complete === true && p5.tasks_live !== false && p5.issues), names = loaded.map((p5) => p5.name), partial = [.../* @__PURE__ */ new Set([...loaded.flatMap((p5) => p5.partial || []), ...view.S.display.all.partial.filter((n3) => names.includes(n3))])], missing = c4.projects.filter((id) => !view.S.projects.some((p5) => views.identity(p5) === id));
              if (missing.length) partial.push("Selected projects unavailable");
              const counted = all.filter((t4) => loaded.some((p5) => p5.root === t4.root)), known = (loaded.length > 0 || ps.length === 0) && !partial.length;
              const errors = ps.filter((p5) => !loaded.includes(p5)).map((p5) => p5.name + ": " + (p5.error || "Project snapshot incomplete or stale"));
              const counts = { counts: view.S.display.states.map(({ id, name }) => ({ state: id, name, n: known ? counted.filter((t4) => t4.state === id).length : null })), open: known ? counted.length : null };
              const listing = c4.group === "combined" ? rows.map((t4) => app.taskRow(t4, true)) : api.unique(ps).map((p5) => {
                const id = views.identity(p5), group = rows.filter((t4) => t4.boardIdentity === id), sources = (loaded.some((q4) => views.identity(q4) === id) ? loaded : ps).filter((q4) => views.identity(q4) === id), reason = sources.flatMap((q4) => [...q4.partial || [], ...q4.complete !== true || q4.tasks_live === false ? [q4.error || "Project snapshot incomplete or stale"] : [], ...view.S.display.all.partial.includes(q4.name) ? ["Project task data incomplete"] : []]).join("; ");
                return group.length ? h5("section", { class: "all-tasks-project", key: id }, api.projectHeader(p5, matching.filter((t4) => t4.boardIdentity === id).length, live.filter((t4) => t4.boardIdentity === id), reason), h5("div", { class: "project-tasks", id: "project-tasks-" + encodeURIComponent(id), hidden: api.model().collapsed.includes(id) }, group.map((t4) => app.taskRow(t4, false)))) : null;
              });
              heldOffer = api.heldScope();
              const heldCount = heldOffer.known ? heldOffer.targets.length : "?";
              const start = { count: heldCount, label: "Start " + heldCount + " held \xB7 " + heldOffer.scope, disabled: !heldOffer.known || !heldOffer.targets.length || all.some((t4) => t4.state === "held" && app.startPending(t4)) };
              return h5(Fragment, null, api.pins(), api.bar(), h5("header", { class: "filter-header tqp" }, h5("div", { class: "state-filters", dangerouslySetInnerHTML: { __html: blocks.counters(counts.counts, "", "Selected projects", partial, c4.states, counts.open, start) } })), api.admissionReview(), errors.map((reason) => h5("p", { class: "meta", role: "status", key: reason }, reason)), app.answerNotices(), rows.length ? listing : h5("div", { class: "body meta" }, !known ? "Tasks unknown" : "No matching tasks", " ", h5("button", { "data-a": "filter-reset" }, "Reset")));
            },
            receive: function(page) {
              const pin = [...pinReads.values()].find((e4) => e4.id === page.id && e4.root === page.root);
              if (!pin?.loading) return;
              ui.cancel(pin.timer);
              pin.loading = false;
              pin.reason = page.reason;
              pin.value = !page.reason && page.tasks?.[0]?.n === pin.task ? page.tasks[0] : null;
              if (!pin.value && !pin.reason) pin.reason = "Board response omitted the task";
              present();
            },
            outcome: function(t4) {
              return module.exports.outcome(t4);
            },
            conditions: function() {
              const c4 = api.criteria(), done = api.current().id === "completed";
              return h5(
                Fragment,
                null,
                h5("fieldset", null, h5("legend", null, "Projects"), h5("div", { class: "facet-list" }, h5("button", { "data-a": "view-facet", "data-facet": "projects", "data-value": "", "aria-pressed": String(!c4.projects.length) }, "All projects"), api.unique(view.S.projects).map((p5) => h5("div", { class: "project-choice", key: views.identity(p5) }, h5("label", null, h5("input", { type: "checkbox", "data-project": views.identity(p5), "aria-label": "Include " + p5.name, disabled: !views.identity(p5), checked: !c4.projects.length || c4.projects.includes(views.identity(p5)) })), h5("button", { "data-a": "view-facet", "data-facet": "projects", "data-value": views.identity(p5), disabled: !views.identity(p5), "aria-label": "Show only " + p5.name, "aria-pressed": String(c4.projects.length === 1 && c4.projects[0] === views.identity(p5)) }, p5.name))))),
                done ? h5(
                  Fragment,
                  null,
                  h5("fieldset", null, h5("legend", null, "Period"), h5("div", { class: "options" }, [["7d", "7 days"], ["30d", "30 days"], ["all", "All time"]].map(([value, label]) => h5("button", { "data-a": "view-period", "data-value": value, "aria-pressed": String(c4.period === value) }, label)))),
                  h5("fieldset", null, h5("legend", null, "Outcome"), h5("div", { class: "facet-list" }, h5("button", { "data-a": "view-facet", "data-facet": "outcomes", "data-value": "", "aria-pressed": String(!c4.outcomes.length) }, "All outcomes"), Object.entries(outcomes).map(([id, [mark, , label]]) => h5("button", { "data-a": "view-facet", "data-facet": "outcomes", "data-value": id, "aria-pressed": String(c4.outcomes.includes(id)) }, icon(mark), label))))
                ) : (
                  // #1700 A4: each state choice keeps its token tone (Review purple), as on the task rows.
                  h5("fieldset", null, h5("legend", null, "States"), h5("div", { class: "facet-list" }, h5("button", { "data-a": "all-tasks", "aria-pressed": String(!c4.states.length) }, "All states"), view.S.display.states.map(({ id: s4, name: n3 }) => h5("button", { "data-a": "view-facet", "data-facet": "states", "data-value": s4, "aria-pressed": String(c4.states.includes(s4)) }, h5("span", { class: blocks.marks[s4][1] || null }, icon(blocks.marks[s4][0])), n3))))
                )
              );
            },
            actions: function() {
              const c4 = api.criteria(), v4 = api.current();
              return h5(
                Fragment,
                null,
                v4.id !== "completed" && h5("fieldset", null, h5("legend", null, "Display"), h5("div", { class: "options" }, [["combined", "All together"], ["project", "By project"]].map(([value, label]) => h5("button", { "data-a": "view-group", "data-value": value, "aria-pressed": String(c4.group === value) }, label)))),
                owned && v4.id === "completed" && h5("button", { "data-a": "completed-open" }, "Read finished work"),
                !owned && api.naming(),
                h5("div", { class: "options" }, h5("button", { "data-a": "view-save", disabled: !views.dirty(v4) }, icon("save"), "Save"), h5("button", { "data-a": "view-save-as", "aria-expanded": String(!!view.naming) }, "Save as\u2026"), h5("button", { "data-a": "view-reset" }, icon("rotate-ccw"), "Reset"), h5("button", { "data-a": "popup-close" }, "Done"))
              );
            },
            naming: function() {
              const v4 = api.current();
              return view.naming && h5("div", { class: "view-naming" }, h5("input", { "data-view-name": "", "aria-label": "New view name", value: v4.draft, maxLength: 80, placeholder: "View name" }), h5("button", { "data-a": "view-add", disabled: !v4.draft.trim() || api.model().views.length >= 64 }, "Create"), h5("button", { "data-a": "view-cancel" }, "Cancel"));
            },
            cancelNaming: function() {
              view.naming = false;
              present();
              menu.querySelector('[data-a="view-save-as"]')?.focus();
            },
            editor: function() {
              return h5(Fragment, null, h5("div", { class: "popover-heading" }, h5("strong", null, "View"), button("x", "Close view editor", { "data-a": "popup-close" })), api.conditions(), api.actions());
            },
            // P-V1: Delete beside each custom view; the shortcut pin stays a separate control; one persisted Undo receipt.
            presets: function() {
              const m5 = api.model();
              return h5(
                owned ? "fieldset" : Fragment,
                null,
                owned ? h5("legend", null, "Saved views") : h5("div", { class: "popover-heading" }, h5("strong", null, "Saved views"), button("x", "Close saved views", { "data-a": "popup-close" })),
                m5.views.map((v4) => h5("div", { class: "saved-view", key: v4.id }, h5("button", { "data-a": "view-select", "data-id": v4.id, "aria-pressed": String(v4.id === api.current().id) }, v4.name, views.dirty(v4) ? " *" : ""), button(v4.pinned ? "pin" : "pin-off", (v4.pinned ? "Remove shortcut for " : "Add shortcut for ") + v4.name, { "data-a": "view-pin", "data-id": v4.id, "aria-pressed": String(v4.pinned) }), !views.builtin(v4.id) && button("trash-2", "Delete view " + v4.name, { "data-a": "view-delete", "data-id": v4.id }))),
                m5.undo && h5("div", { class: "view-receipt", role: "status" }, "Deleted ", h5("strong", null, m5.undo.view.name), button("undo-2", "Undo deleting " + m5.undo.view.name, { "data-a": "view-undo" })),
                owned ? api.naming() : h5(Fragment, null, api.naming(), h5("button", { "data-a": "view-save-as", "aria-expanded": String(!!view.naming) }, "Save as\u2026")),
                view.viewError && h5("p", { role: "alert" }, view.viewError)
              );
            },
            // #1781: details carry a short question cue; the full question and options read in Conversations (input-scroll 04).
            question: function(t4) {
              if (t4.state !== "ask") return t4.card && h5("p", { class: "task-card-text" }, t4.card);
              const q4 = t4.question, text = (q4?.parsed ? q4.problem : q4?.text) || t4.card || "Open the question to read and answer.";
              return h5("p", { class: "task-card-text question-cue" }, text.length > 170 ? text.slice(0, 167) + "\u2026" : text);
            },
            // § 2 visual evidence: ask images as thumbnails that open full size; the link alone without the service's copy.
            images: function(q4) {
              const im = (q4?.evidence || []).filter((e4) => e4.image);
              return im.length > 0 && h5("p", { class: "task-images" }, im.map((e4) => h5("a", { key: e4.url, href: e4.url, "data-url": e4.url, title: "Open " + e4.label + " full size" }, /^data:image\/(png|jpeg|gif|webp);base64,/.test(e4.src || "") ? h5("img", { class: "task-thumb", src: e4.src, alt: e4.label }) : e4.label + " \u2197")));
            },
            // M3/MP13: every open task has one detail surface in the main list scroll.
            attention: function(t4) {
              const im = t4.state === "ask" && api.images(t4.question), open = view.popup?.kind === "task" && view.popup.key === t4.key;
              return h5(Fragment, null, im, open && h5("section", { id: "taskq-question-details", class: "task-inline-details question-task-details " + (blocks.marks[t4.state]?.[1] || ""), tabindex: -1, "aria-label": "Task details for #" + t4.n }, api.detail(t4)));
            },
            detail: function(t4) {
              if (!t4) return null;
              const p5 = view.S.projects.find((p6) => p6.root === t4.root), pinned = views.pinned(api.model(), api.ref(t4));
              return h5(
                Fragment,
                null,
                h5("div", { class: "popover-heading" }, app.answerSlot(t4, false), button(pinned ? "pin-off" : "pin", pinned ? "Unpin task" : "Pin task", { "data-a": "task-pin", "aria-pressed": String(pinned) }), button("x", "Close task details", { "data-a": "popup-close" })),
                t4.wait && h5("p", { role: "status" }, t4.wait),
                t4.publication?.reason && !t4.wait && h5("p", { role: "status" }, t4.publication.reason),
                t4.enqueued && !t4.publication?.reason && h5("p", { class: "meta" }, "CI progress unknown in this snapshot."),
                t4.failed && h5("p", { class: "alert" }, t4.failed),
                t4.later && !t4.until && h5("p", { role: "status" }, t4.later),
                api.question(t4),
                t4.result && h5("p", { class: "task-card-text" }, "Result: " + t4.result),
                require_task_health().details(t4),
                h5("div", { class: "task-destinations" }, blocks.destinations(t4).map((d5) => d5.url ? h5("a", { href: d5.url, "data-url": d5.url, "data-destination": d5.kind }, icon(d5.kind === "native" ? "external-link" : "network"), d5.label) : h5("p", { class: "meta", "data-destination": d5.kind }, d5.label + " unavailable: " + d5.reason))),
                (t4.blocked || []).map((b5) => h5("p", null, h5("a", { href: b5.url, "data-url": b5.url }, b5.ref), " \xB7 " + b5.reason)),
                h5("div", { class: "row-actions" }, t4.state === "held" && app.startButton([t4]), h5("span", { dangerouslySetInnerHTML: { __html: require_task_controls().render(t4, { key: t4.key, url: t4.url, board: p5.board_kind, except: ["start"] }) } })),
                h5(
                  "details",
                  { class: "task-more", "data-view-key": "more:" + t4.key },
                  h5("summary", null, icon("info"), "Task info"),
                  h5(
                    "dl",
                    null,
                    h5("dt", null, "Runtime"),
                    h5("dd", null, t4.runtime || "unknown"),
                    h5("dt", null, "Host"),
                    h5("dd", null, t4.host || "unknown"),
                    h5("dt", null, "Role"),
                    h5("dd", null, t4.role || "unknown"),
                    h5("dt", null, "Session"),
                    h5("dd", null, t4.session || t4.session_reason || "Session unavailable"),
                    h5("dt", null, "Attempt"),
                    h5("dd", null, t4.attempt || "unknown"),
                    h5("dt", null, "Revision"),
                    h5("dd", null, t4.revision ?? "unknown"),
                    h5("dt", null, "Total age"),
                    h5("dd", null, t4.total_age ?? "unknown"),
                    h5("dt", null, "Steps"),
                    h5("dd", null, Array.isArray(t4.steps) ? t4.steps.filter((s4) => s4.complete === true).length + " completed" : "unknown")
                  ),
                  Array.isArray(t4.steps) && t4.steps.length ? h5("ol", { class: "task-steps" }, t4.steps.map((s4) => h5("li", null, s4.state, " \xB7 ", s4.age ?? "duration unknown", s4.complete ? "" : " \xB7 current"))) : h5("p", { class: "meta" }, "Ordered stage history unavailable in this snapshot.")
                ),
                app.answerNotices(p5.issues)
              );
            },
            // P-W4: the menu takes the View window's model whole; a switched view restores its own scroll and open task.
            adopt: function(value) {
              const prior = view.dashboard?.selected;
              view.dashboard = views.normalize(value);
              if (prior !== view.dashboard.selected) {
                view.restoreScroll = api.current().scroll;
                restore = true;
              }
              present();
            },
            popup: function() {
              const p5 = view.popup;
              if (!p5 || p5.kind === "task") return null;
              return popover.render({ label: p5.kind === "editor" ? "View editor" : "Saved views", children: p5.kind === "editor" ? api.editor() : api.presets() });
            },
            place: function() {
              if (restore && view.S && view.page === "home") {
                restore = false;
                const key = api.current().open, trigger = key && menu.querySelector('[data-a="task-detail"][data-key="' + CSS2.escape(key) + '"]');
                if (!view.popup && trigger) {
                  api.open("task", trigger, key, false);
                  return;
                }
              }
              if (view.popup) {
                const selector = view.popup.kind === "task" ? '[data-a="task-detail"][data-key="' + CSS2.escape(view.popup.key) + '"]' : '[data-a="' + (view.popup.kind === "views" ? "view-list" : "view-editor") + '"]', trigger = menu.querySelector(selector);
                if (trigger) view.popup.anchor = api.anchor(trigger);
              }
              for (const b5 of menu.querySelectorAll('[data-a="task-detail"]')) {
                b5.setAttribute("aria-expanded", String(view.popup?.kind === "task" && view.popup.key === b5.dataset.key));
                b5.setAttribute("aria-controls", "taskq-question-details");
                b5.removeAttribute("aria-haspopup");
              }
              if (view.popup && view.popup.kind !== "task") popover.place(menu.querySelector("#taskq-popover"), view.popup.anchor);
            },
            click: function(e4) {
              const link = e4.target.closest("[data-pin-open]");
              if (link && pinReads.get(link.dataset.pinOpen)?.value?.state === "closed") {
                api.unpin(link.dataset.pinOpen);
                ui.defer(present, 0);
                return false;
              }
              const b5 = e4.target.closest("button");
              if (!b5 || b5.disabled) return false;
              const d5 = b5.dataset, m5 = api.model();
              if (d5.a === "search-open") {
                view.searchOpen = !!api.criteria().query || !view.searchOpen;
                present();
                menu.querySelector(view.searchOpen ? "[data-task-query]" : "[data-a=search-open]")?.focus({ preventScroll: true });
                return true;
              }
              if (d5.a === "search-clear") {
                api.criteria().query = "";
                view.searchOpen = false;
                present();
                menu.querySelector("[data-a=search-open]")?.focus({ preventScroll: true });
                return true;
              }
              if (d5.a === "held-review") {
                if (heldOffer?.known && heldOffer.targets.length) {
                  heldReview = structuredClone(heldOffer);
                  present();
                  menu.querySelector("[data-a=held-confirm]")?.focus();
                }
                return true;
              }
              if (d5.a === "held-cancel") {
                heldReview = null;
                present();
                menu.querySelector("[data-a=held-review]")?.focus();
                return true;
              }
              if (d5.a === "held-confirm") {
                if (!heldReview || heldReview.sent) return true;
                const current = api.heldScope();
                if (!current.known || current.signature !== heldReview.signature || !app.startHeld(heldReview.targets)) heldReview.error = "Held targets or scope changed; cancel and review the current drafts.";
                else heldReview.sent = true;
                present();
                return true;
              }
              if (d5.a === "project-collapse") {
                const closing = !m5.collapsed.includes(d5.project);
                m5.collapsed = closing ? [...m5.collapsed, d5.project] : m5.collapsed.filter((id) => id !== d5.project);
                if (closing && view.popup?.kind === "task" && api.live().some((t4) => t4.key === view.popup.key && t4.boardIdentity === d5.project)) api.close(false);
                else present();
                return true;
              }
              if (d5.a === "task-detail") {
                if (view.popup?.kind === "task" && view.popup.key === d5.key) api.close();
                else api.open("task", b5, d5.key, true);
                return true;
              }
              if ((d5.a === "view-editor" || d5.a === "view-list") && view.viewsWindow) {
                send({ a: "views-open" });
                return true;
              }
              if (d5.a === "view-editor" || d5.a === "view-list") {
                if (view.popup?.kind === (d5.a === "view-editor" ? "editor" : "views")) api.close();
                else api.open(d5.a === "view-editor" ? "editor" : "views", b5);
                return true;
              }
              if (d5.a === "popup-close") {
                if (owned) send({ a: "hide" });
                else api.close();
                return true;
              }
              if (d5.a === "task-pin") {
                const t4 = api.live().find((t5) => t5.key === view.popup?.key);
                if (t4) views.toggle(m5, { ref: api.ref(t4), title: t4.title, url: t4.url });
                api.close();
                return true;
              }
              if (d5.a === "task-unpin") {
                api.unpin(d5.ref);
                present();
                return true;
              }
              if (d5.a === "pin-retry") {
                api.pinRead({ ref: d5.ref }, true);
                present();
                return true;
              }
              if (d5.a === "completed-open") {
                api.openCompleted();
                return true;
              }
              if (d5.a === "pin-transcript") {
                const entry = pinReads.get(d5.ref);
                if (entry?.value?.state === "closed") {
                  api.openCompleted(entry);
                  api.unpin(d5.ref);
                  present();
                }
                return true;
              }
              if (d5.a === "view-select" && d5.id === "completed" && owned) {
                view.completedEditor = true;
                present();
                return true;
              }
              if (d5.a === "view-select" && d5.id === "completed") {
                api.close(false);
                const completed = api.model().views.find((v4) => v4.id === "completed");
                send({ a: "workspace", open: { page: "conversations", filter: "finished", criteria: completed?.working || api.criteria() } });
                return true;
              }
              if (d5.a === "view-select") {
                view.completedEditor = false;
                if (!owned) api.current().scroll = menu.querySelector(".content")?.scrollTop || 0;
                m5.selected = d5.id;
                view.popup = null;
                if (d5.id === "completed") api.openCompleted();
                view.restoreScroll = api.current().scroll;
                restore = true;
                present();
                menu.querySelector('[data-a="view-list"]')?.focus();
                return true;
              }
              if (d5.a === "view-pin") {
                const v4 = m5.views.find((v5) => v5.id === d5.id);
                if (v4) v4.pinned = !v4.pinned;
                present();
                return true;
              }
              if (d5.a === "view-delete") {
                if (views.remove(m5, d5.id)) {
                  if (m5.selected === "all") view.restoreScroll = api.current().scroll;
                  present();
                  menu.querySelector((owned ? "" : "#taskq-popover ") + '[data-a="view-undo"]')?.focus();
                }
                return true;
              }
              if (d5.a === "view-undo") {
                views.undo(m5);
                present();
                menu.querySelector((owned ? "" : "#taskq-popover ") + '[data-a="view-select"][aria-pressed="true"]')?.focus();
                return true;
              }
              if (d5.a === "view-save-as") {
                view.naming = true;
                present();
                menu.querySelector("[data-view-name]")?.focus();
                return true;
              }
              if (d5.a === "view-cancel") {
                api.cancelNaming();
                return true;
              }
              if (d5.a === "view-add") {
                if (views.add(m5, api.current().draft, "view-" + Array.from(crypto2.getRandomValues(new Uint8Array(16)), (b6) => b6.toString(16).padStart(2, "0")).join(""))) {
                  view.naming = false;
                  view.popup = null;
                  present();
                  menu.querySelector('[data-a="view-select"][aria-pressed="true"], [data-a="view-list"]')?.focus();
                }
                return true;
              }
              if (d5.a === "view-facet") {
                const value = d5.value, key = d5.facet, current = api.criteria()[key];
                if (!["states", "projects", "outcomes"].includes(key)) return true;
                api.set({ [key]: !value ? [] : key === "projects" ? [value] : current.includes(value) ? current.filter((x5) => x5 !== value) : [...current, value] });
                return true;
              }
              if (d5.a === "view-group") {
                api.set({ group: d5.value });
                return true;
              }
              if (d5.a === "view-period") {
                api.set({ period: d5.value });
                return true;
              }
              if (d5.a === "view-save") {
                views.save(m5);
                present();
                return true;
              }
              if (d5.a === "view-reset" || d5.a === "filter-reset") {
                views.reset(m5);
                present();
                return true;
              }
              if (d5.a === "all-tasks") {
                api.set({ states: [] });
                return true;
              }
              return false;
            },
            mount: function() {
              ui.listen(menu, "focusin", (e4) => {
                if (e4.target.hasAttribute("data-task-query")) view.searchOpen = true;
              });
              ui.listen(menu, "focusout", (e4) => {
                if (e4.target.hasAttribute("data-task-query")) ui.defer(() => {
                  if (!api.criteria().query && view.searchOpen && !document2.activeElement?.closest(".task-search")) {
                    view.searchOpen = false;
                    present();
                  }
                }, 0);
              });
              ui.listen(document2, "keydown", (e4) => {
                if (e4.key === "Escape" && e4.target.hasAttribute?.("data-task-query")) {
                  e4.preventDefault();
                  e4.stopImmediatePropagation();
                  if (!api.criteria().query) view.searchOpen = false;
                  present();
                  menu.querySelector("[data-a=search-open]")?.focus({ preventScroll: true });
                }
              }, true);
              ui.listen(menu, "change", (e4) => {
                if (!e4.target.hasAttribute("data-project")) return;
                const value = e4.target.dataset.project, current = api.criteria().projects, selected = current.length ? current : api.unique(view.S.projects).map(views.identity).filter(Boolean);
                api.set({ projects: e4.target.checked ? [...selected, value] : selected.filter((x5) => x5 !== value) });
              });
              ui.listen(document2, "keydown", (e4) => {
                if (e4.key === "Escape" && view.naming) {
                  e4.preventDefault();
                  e4.stopImmediatePropagation();
                  api.cancelNaming();
                }
              }, true);
              ui.listen(menu, "input", (e4) => {
                if (e4.target.hasAttribute("data-task-query")) {
                  api.criteria().query = e4.target.value;
                  present();
                }
                if (e4.target.hasAttribute("data-view-name")) {
                  api.current().draft = e4.target.value.slice(0, 80);
                  present();
                }
              });
              ui.listen(document2, "pointerdown", (e4) => {
                if (view.popup && view.popup.kind !== "task" && !e4.target.closest('#taskq-popover,#taskq-question-details,[data-a="task-detail"],[data-a="view-editor"],[data-a="view-list"]')) api.close(false);
              });
              ui.listen(window2, "resize", api.place);
              ui.listen(menu, "scroll", api.place, true);
            }
          });
          return api;
        }
      });
    }
  });

  // src/reports.js
  var require_reports = __commonJS({
    "src/reports.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      if (typeof window !== "undefined") window.Prism = { manual: true, disableWorkerMessageHandler: true };
      var Prism2 = require_prism_core();
      require_prism_markup();
      require_prism_css();
      require_prism_clike();
      require_prism_javascript();
      require_prism_rust();
      require_prism_python();
      require_prism_bash();
      require_prism_json();
      require_prism_typescript();
      require_prism_diff();
      require_prism_sql();
      require_prism_yaml();
      var syntaxCache = /* @__PURE__ */ new Map();
      var card = require_answer_card();
      var composer = require_pm_composer().composer;
      var time = require_relative_time();
      var icons = require_ui_icons2();
      var dashboard = require_task_dashboard();
      var { create, body } = execution.module("reports", {
        create: function({ send = ui.send, draw, defer = ui.defer, onOpen, onMove, navigation = () => ({ index: 0, length: 1 }), reading = () => {
        }, transcript, id = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join("") }) {
          let projects = [], pages = /* @__PURE__ */ new Map(), pending = /* @__PURE__ */ new Map(), selected = null, notice = "", draft = null, successor = null, readFailure = "", readQueue = [], reasonField = null;
          const textPages = /* @__PURE__ */ new Map(), bodies = /* @__PURE__ */ new Map(), receipts = /* @__PURE__ */ new Map(), outcomes = /* @__PURE__ */ new Map(), key = (root, n3) => root + "#" + n3;
          const saved = {}, controllers = /* @__PURE__ */ new Map(), displayedReads = /* @__PURE__ */ new Set();
          const settled = (r4) => ["accepted", "drop", "made", "answered"].includes(r4.decision);
          const namespace = (r4, kind) => "result:" + (projects.find((p5) => p5.root === r4.root)?.issues || r4.root) + "#" + r4.n + "@" + r4.sha + ":" + kind;
          const receiptKey = (root, n3, sha) => key(root, n3) + "@" + sha;
          const active = (op) => selected?.root === op.root && selected.n === op.n && selected.sha === op.sha;
          function confirmed(op, r4) {
            return r4.report?.n === op.n && r4.report?.sha === op.sha && (op.action === "read" ? r4.report.read : op.action === "open" ? true : op.action === "accept" ? r4.report.decision === "accepted" : op.action === "later" ? r4.report.decision === "later" && (!op.until || r4.report.later_until === op.until) : op.action === "drop" ? r4.report.decision === "drop" : op.action === "make" ? r4.report.decision === "made" : r4.successor > 0);
          }
          function outcome(op, text, next, shown = active(op)) {
            const k5 = receiptKey(op.root, op.n, op.sha), prior = outcomes.get(k5) || {}, value = { notice: text === void 0 ? prior.notice || "" : text, successor: next === void 0 ? prior.successor || null : next };
            outcomes.set(k5, value);
            if (shown) {
              notice = value.notice;
              successor = value.successor;
            }
          }
          function targets(ps) {
            const seen = /* @__PURE__ */ new Set();
            return ps.filter((p5) => {
              const board = p5.issues || p5.root;
              if (seen.has(board)) return false;
              seen.add(board);
              return true;
            });
          }
          function request(root, action, extra = {}, resolve) {
            if ([...pending.values()].some((p5) => p5.id !== extra.id && p5.root === root && p5.n === extra.n && p5.sha === extra.sha && (action === "read" || p5.action !== "read"))) {
              resolve?.({ state: "unknown", reason: "Another result operation needs readback" });
              return;
            }
            const op = { id: id(), root, action, ...extra };
            pending.set(op.id, op);
            send({ a: "reports", ...op });
            if (resolve) op.resolve = resolve;
            defer(() => {
              if (pending.get(op.id) !== op) return;
              op.unknown = true;
              if (["read", "open"].includes(op.action)) {
                pending.delete(op.id);
                readFailure = "Result read outcome unknown; waiting for service readback.";
              }
              op.resolve?.({ state: "unknown", reason: "No bridge receipt received; check the same operation" });
              op.resolve = null;
              outcome(op, "Result outcome unknown; waiting for service readback.");
              api.readNext();
              draw();
            }, action === "open" ? 3e4 : 18e4);
          }
          function cacheBody(r4) {
            const k5 = receiptKey(r4.root, r4.n, r4.sha);
            bodies.delete(k5);
            bodies.set(k5, { root: r4.root, n: r4.n, sha: r4.sha, body: r4.body, verdict: r4.verdict, tasks: r4.tasks, lazy: false });
            let bytes = 0;
            for (const [key2, row] of [...bodies].reverse()) {
              bytes += new TextEncoder().encode(JSON.stringify(row)).length;
              if ((bodies.size > 2 || bytes > 2 * 1024 * 1024) && !active(row)) bodies.delete(key2);
            }
            for (const [key2, c4] of controllers) if (controllers.size > 16 && !active(c4) && ![...pending.values()].some((op) => op.root === c4.root && op.n === c4.n && op.sha === c4.sha)) controllers.delete(key2);
          }
          function readShown(r4) {
            const k5 = receiptKey(r4.root, r4.n, r4.sha);
            if (!r4.lazy && (r4.body || r4.kind === "task") && !r4.read && !displayedReads.has(k5)) {
              displayedReads.add(k5);
              request(r4.root, "read", { n: r4.n, sha: r4.sha });
            }
          }
          function summary(text, heading) {
            const lines = (text || "").split("\n"), start = lines.findIndex((l5) => l5.replace(/^#+\s*/, "").trim().toLowerCase() === heading.toLowerCase());
            if (start < 0) return "";
            const rest = lines.slice(start + 1), end = rest.findIndex((l5) => /^#{1,6}\s/.test(l5));
            return rest.slice(0, end < 0 ? void 0 : end).join("\n").trim();
          }
          function textPage(r4, source) {
            const key2 = receiptKey(r4.root, r4.n, r4.sha) + ":" + source;
            let page = textPages.get(key2);
            if (!page) {
              const match = new RegExp("^report:" + source + "-(\\d+):").exec(navigation(r4).position?.key || "");
              page = { start: match ? Number(match[1]) : 0, open: false };
            }
            textPages.delete(key2);
            textPages.set(key2, page);
            while (textPages.size > 24) textPages.delete(textPages.keys().next().value);
            return page;
          }
          function textEnd(text, start) {
            let end = Math.min(text.length, start + 32768), count = 0;
            for (const match of text.slice(start, end).matchAll(/\n\s*\n/g)) if (++count === 63) {
              end = start + match.index + match[0].length;
              break;
            }
            if (end < text.length && /[\uD800-\uDBFF]/.test(text[end - 1])) end++;
            return end;
          }
          function textBody(r4, text, source) {
            const page = textPage(r4, source), start = page.start, end = textEnd(text, start), key2 = source + (start ? "-" + start : "");
            const turn = (next) => {
              page.start = next;
              reading("report:" + source + (next ? "-" + next : "") + ":0", r4);
              draw();
            };
            return [body(text.slice(start, end), key2), (start > 0 || end < text.length) && h5("div", { class: "conversation-tools" }, h5("span", { class: "meta" }, "Text " + (start + 1) + "\u2013" + end + " / " + text.length), start > 0 && h5("button", { onClick: () => {
              let before = 0;
              for (let at = 0; at < start; ) {
                before = at;
                at = textEnd(text, at);
              }
              turn(before);
            } }, "Previous text"), end < text.length && h5("button", { onClick: () => turn(end) }, "Next text"))];
          }
          function field(text, label) {
            const lines = (text || "").split("\n"), start = lines.findIndex((l5) => l5.startsWith(label + ":"));
            if (start < 0) return "Unknown: not recorded in the accepted hand-in.";
            const rest = lines.slice(start + 1), end = rest.findIndex((l5) => /^(Changed|Checks|Open|Report):/.test(l5) && !l5.startsWith(label + ":"));
            return [lines[start], ...rest.slice(0, end < 0 ? void 0 : end)].map((l5) => l5.startsWith(label + ":") ? l5.slice(label.length + 1).trim() : l5).join("\n");
          }
          function remember(op, report) {
            const k5 = receiptKey(op.root, op.n, op.sha), won = receipts.get(k5);
            const collected = pages.get(op.root)?.rows.find((r4) => r4.n === op.n && r4.sha === op.sha);
            if (Number.isSafeInteger(collected?.comments_through) && (!Number.isSafeInteger(report.comments_through) || collected.comments_through > report.comments_through)) return collected;
            if (won?.sha === op.sha && won.visible === false) return won;
            const fields = { sha: report.sha };
            for (const field2 of ["read", "decision", "visible", "pending_decision", "tasks_made", "comments_through", "later_until"]) if (Object.hasOwn(report, field2)) fields[field2] = report[field2];
            if (op.action === "read" && won?.sha === op.sha) {
              if (["later", "accepted"].includes(won.decision)) fields.decision = won.decision;
              if (won.decision === "later") fields.later_until = won.later_until;
              if (won.pending_decision === "make") fields.pending_decision = "make";
            }
            if (Number.isSafeInteger(won?.comments_through) && Number.isSafeInteger(fields.comments_through)) fields.comments_through = Math.max(won.comments_through, fields.comments_through);
            const merged = { ...won?.sha === op.sha ? won : {}, ...fields };
            receipts.set(k5, merged);
            return merged;
          }
          const api = execution.module("reports.view", {
            sync(ps, spare = 16 * 1024 * 1024) {
              projects = targets(ps);
              let retainedBytes = 0, retainedRows = 0;
              for (const root of pages.keys()) if (!projects.some((p5) => p5.root === root)) pages.delete(root);
              for (const p5 of projects) {
                const data = p5.reports;
                const incoming = [...data?.reports || [], ...data?.history || []], retained = data?.complete === true ? [] : (pages.get(p5.root)?.rows || []).filter((r4) => !incoming.some((v4) => v4.n === r4.n) && (active(r4) || ++retainedRows <= 192 && (retainedBytes += new TextEncoder().encode(JSON.stringify(r4)).length) <= spare / 2));
                pages.set(p5.root, { rows: [...incoming, ...retained].map((r4) => {
                  const k5 = receiptKey(p5.root, r4.n, r4.sha), won = receipts.get(k5);
                  if (Number.isSafeInteger(r4.comments_through) && Number.isSafeInteger(won?.comments_through) && r4.comments_through >= won.comments_through) receipts.delete(k5);
                  return receipts.has(k5) ? { ...r4, ...won } : r4;
                }), unknown: data?.complete !== true, reason: data?.reason || (data?.complete === true ? "" : "Reports not collected yet") });
              }
              if (selected) {
                const row = api.rows(true).find((r4) => key(r4.root, r4.n) === key(selected.root, selected.n) && r4.sha === selected.sha);
                selected = row ? { ...selected, ...row, ...bodies.get(receiptKey(row.root, row.n, row.sha)) } : null;
              }
              for (const op of pending.values()) if (op.unknown && !op.checking && !pages.get(op.root)?.unknown) {
                const row = pages.get(op.root)?.rows.find((r4) => r4.n === op.n && r4.sha === op.sha);
                if (row && (confirmed(op, { report: row }) || settled(row) && ["accept", "later", "drop", "make"].includes(op.action))) {
                  op.checking = true;
                  defer(() => api.receive({ id: op.id, root: op.root, report: row }), 0);
                }
              }
            },
            bounds() {
              return { bodySources: bodies.size, bodyBytes: [...bodies.values()].reduce((n3, r4) => n3 + new TextEncoder().encode(JSON.stringify(r4)).length, 0), controllers: controllers.size, textPages: textPages.size };
            },
            refresh() {
              api.sync(projects);
              draw();
            },
            rows(history2 = false) {
              return projects.flatMap((p5) => (pages.get(p5.root)?.rows || []).filter((r4) => history2 || r4.visible !== false).map((r4) => ({ ...r4, root: p5.root }))).sort((a4, b5) => (b5.closed || 0) - (a4.closed || 0) || b5.n - a4.n);
            },
            unread() {
              return api.rows().filter((r4) => !r4.read);
            },
            // R6 Decide: mirrors reports::decide; an undated or past Later returns, the oldest close first.
            undecided(r4) {
              const until = Date.parse(r4.later_until || "");
              return r4.kind !== "task" && r4.visible !== false && !r4.pending_decision && (!r4.decision || r4.decision === "none" || r4.decision === "later" && !(until > Date.now()));
            },
            decide(rows = api.rows()) {
              return rows.filter(api.undecided).sort((a4, b5) => (a4.closed || 0) - (b5.closed || 0) || a4.n - b5.n);
            },
            count() {
              return projects.every((p5) => pages.has(p5.root) && !pages.get(p5.root).unknown) ? api.unread().length : null;
            },
            sources() {
              return projects.map((p5) => ({ root: p5.root, name: p5.name, loaded: api.rows(true).filter((r4) => r4.root === p5.root).length, reason: pages.get(p5.root)?.unknown ? pages.get(p5.root).reason : "" }));
            },
            reason() {
              return projects.flatMap((p5) => pages.get(p5.root)?.reason ? [p5.name + ": " + pages.get(p5.root).reason] : []).join("; ");
            },
            current() {
              return selected;
            },
            feedback() {
              return notice;
            },
            readFailure() {
              return readFailure;
            },
            markRead(rows) {
              readFailure = "";
              const seen = new Set([...readQueue, ...pending.values()].map((r4) => receiptKey(r4.root, r4.n, r4.sha)));
              for (const r4 of rows) if (!seen.has(receiptKey(r4.root, r4.n, r4.sha))) {
                readQueue.push(r4);
                seen.add(receiptKey(r4.root, r4.n, r4.sha));
              }
              api.readNext();
            },
            // M14: serialize bulk reads through the existing bounded native command queue.
            readNext() {
              if ([...pending.values()].some((p5) => p5.action === "read")) return;
              const r4 = readQueue.shift();
              if (r4) request(r4.root, "read", { n: r4.n, sha: r4.sha });
            },
            open(root, n3, sha) {
              return execution.action("reports.card.open", () => {
                const r4 = api.rows(true).find((r5) => r5.root === root && r5.n === n3 && (!sha || r5.sha === sha));
                if (!r4) return false;
                selected = { ...r4, ...bodies.get(receiptKey(root, n3, r4.sha)) };
                if (r4.lazy && !selected.body) request(root, "open", { n: n3, sha: r4.sha });
                displayedReads.delete(receiptKey(root, n3, r4.sha));
                const prior = outcomes.get(receiptKey(root, n3, r4.sha));
                notice = prior?.notice || "";
                draft = null;
                successor = prior?.successor || api.savedSuccessor(r4);
                onOpen?.(selected);
                draw();
                readShown(r4);
                return true;
              })();
            },
            savedSuccessor(r4) {
              const n3 = Object.values(saved).find((v4) => v4.request?.root === r4.root && v4.request?.n === r4.n && v4.request?.sha === r4.sha && v4.receipt?.state === "accepted" && v4.successor > 0)?.successor;
              return n3 ? { n: n3, url: r4.url?.replace(/\d+$/, String(n3)) } : null;
            },
            focus(root, n3) {
              const r4 = api.rows(true).find((r5) => r5.root === root && r5.n === n3);
              if (!r4 || active(r4)) return;
              selected = { ...r4, ...bodies.get(receiptKey(root, n3, r4.sha)) };
              if (r4.lazy && !selected.body) request(root, "open", { n: n3, sha: r4.sha });
              const prior = outcomes.get(receiptKey(root, n3, r4.sha));
              notice = prior?.notice || "";
              successor = prior?.successor || api.savedSuccessor(r4);
            },
            move(step) {
              onMove?.(step);
            },
            act(action, text, until) {
              if (!selected) return;
              outcome(selected, "", null);
              request(selected.root, action, { n: selected.n, sha: selected.sha, ...text === void 0 ? {} : { text }, ...until === void 0 ? {} : { until } });
              draw();
            },
            receive(r4) {
              const op = pending.get(r4.id);
              if (!op || op.root !== r4.root) return false;
              const shown = active(op);
              const reply = { state: r4.reason ? r4.receipt?.outcome === "rejected" ? "rejected" : r4.receipts?.length ? "partial" : "unknown" : "accepted", reason: r4.reason };
              if (r4.report?.n !== op.n || r4.report?.sha !== op.sha) {
                reply.state = "unknown";
                reply.reason = r4.reason || "No matching result receipt";
              } else if (!r4.reason && !confirmed(op, r4)) {
                reply.state = "rejected";
                reply.reason = "The displayed action was not confirmed; review the current result decision.";
              }
              if (!["read", "open"].includes(op.action) && ["unknown", "partial"].includes(reply.state)) op.unknown = true;
              else pending.delete(r4.id);
              for (const [k5, v4] of Object.entries(saved)) if (v4.op?.id === r4.id && v4.request?.root === r4.root) {
                saved[k5] = { ...v4, receipt: reply, ...reply.state === "accepted" && v4.op.text ? { draft: "" } : {}, ...reply.state === "accepted" && r4.successor > 0 ? { successor: r4.successor } : {} };
                send({ a: "answer-local", key: k5, local: saved[k5] });
              }
              op.resolve?.(reply);
              op.resolve = null;
              for (const c4 of controllers.values()) c4.ui.receiveReceipt(c4.board, r4.id, reply);
              if (reply.state !== "accepted") {
                if (op.action === "read") readFailure = reply.reason;
                if (r4.report?.n === op.n && r4.report.sha === op.sha) remember(op, r4.report);
                const made = Array.isArray(r4.receipts) ? r4.receipts : [], uncertain = r4.receipt?.task;
                outcome(op, reply.reason + (made.length ? " \xB7 Created held drafts: " + made.map((n3) => "#" + n3).join(", ") : "") + (uncertain > 0 && !made.includes(uncertain) ? " \xB7 Unverified draft: #" + uncertain : ""), void 0, shown);
                api.readNext();
                draw();
                return true;
              }
              if (r4.report?.n === op.n && r4.report.sha === op.sha) {
                const p5 = pages.get(r4.root), report = remember(op, r4.report);
                if (op.action === "open" && r4.report.body !== void 0) {
                  const body2 = { ...r4.report, root: r4.root, lazy: false };
                  cacheBody(body2);
                  if (shown) selected = { ...selected, ...body2 };
                }
                if (p5) p5.rows = p5.rows.map((row) => row.n !== op.n || row.sha !== op.sha ? row : { ...row, ...report, closed: row.closed });
                if (selected && selected.sha === op.sha && key(selected.root, selected.n) === key(r4.root, op.n)) selected = { ...selected, ...report, closed: selected.closed };
                if (op.action !== "read") outcome(op, report.visible === false ? report.decision === "made" ? "Created held drafts: " + (report.tasks_made || []).map((n3) => "#" + n3).join(", ") : "Report dropped." : op.action === "later" ? op.until ? "Later: back in Decide on " + op.until.slice(0, 10) + "." : "Kept in Reports for later." : "", void 0, shown);
                if (r4.successor > 0) {
                  outcome(op, "Created held draft. Read it back and start it separately.", { n: r4.successor, url: r4.successor_url || (typeof r4.report.url === "string" ? r4.report.url.replace(/\d+$/, String(r4.successor)) : null) }, shown);
                  if (shown && draft === op.action) draft = null;
                }
                if (op.action === "accept") outcome(op, "Result accepted.", void 0, shown);
              } else {
                if (op.action === "read") readFailure = "Result read receipt unknown; waiting for service readback.";
                outcome(op, "Report receipt unknown; waiting for service readback.", void 0, shown);
              }
              api.readNext();
              draw();
              if (op.action === "open" && shown) readShown(selected);
              return true;
            },
            click(b5, ps) {
              const action = b5.dataset.report;
              if (!action) return false;
              if (action === "source-retry" && selected) {
                request(selected.root, "open", { n: selected.n, sha: selected.sha });
                draw();
              } else if (action === "open") api.open(b5.dataset.root, Number(b5.dataset.n));
              else if (action === "reason-send") api.reasonSend();
              else if (action === "date-send" && selected) {
                const k5 = namespace(selected, "decision");
                controllers.get(k5)?.ui.act(k5, selected.n, "submit");
              } else if (action === "date-cancel" && selected) {
                const k5 = namespace(selected, "decision");
                controllers.get(k5)?.ui.act(k5, selected.n, "cancel-result");
              } else if (action === "reason-cancel" && selected) {
                const k5 = namespace(selected, "correction");
                controllers.get(k5)?.ui.act(k5, selected.n, "cancel-result");
              } else if (action === "unresolved") onOpen?.({ next: true });
              else if (action === "previous") api.move(-1);
              else if (action === "next") api.move(1);
              return true;
            },
            threads() {
              return api.rows(true).map((r4) => ({ id: "task:" + (projects.find((p5) => p5.root === r4.root)?.issues || r4.root) + "/" + r4.n, root: r4.root, board: projects.find((p5) => p5.root === r4.root)?.issues, conversation: r4.conversation, task: r4.n, report: r4.n, title: r4.title, result: r4.result, updated: r4.closed * 1e3, finished: true, needsDecision: api.undecided(r4), status: { accepted: "Accepted", drop: "Dropped", made: "Drafts created", answered: "Decided" }[r4.decision] || (r4.kind === "task" ? "Finished" : "Report"), unread: !r4.read, outcome: r4.outcome, closed: r4.closed, marker: api.undecided(r4) ? "Needs decision" : !r4.read ? "New" : "" }));
            },
            hydrate(locals) {
              for (const [k5, v4] of Object.entries(locals || {})) if (k5.startsWith("result:") && !saved[k5] && typeof v4.draft === "string") {
                saved[k5] = { ...v4 };
                if (v4.op && ["sending", "unknown", "partial"].includes(v4.receipt?.state)) {
                  saved[k5].receipt = { state: "unknown", reason: "Workspace reopened; check the original operation" };
                  if (v4.request?.id === v4.op.id && typeof v4.request.root === "string" && Number.isSafeInteger(v4.request.n) && typeof v4.request.sha === "string") pending.set(v4.op.id, { ...v4.request, unknown: true });
                }
              }
            },
            cardRow(r4, kind) {
              const choices = kind === "decision" ? [["Accept", "+ Acknowledge this result; - no new work.", "accept"], ["Later", r4.kind === "task" ? "+ Keep it for another decision; - revisit it yourself." : "+ Return to Decide on a date; - choose the date before Send.", "later"], ["Drop", "+ Dismiss this decision; - no follow-up work.", "drop"]] : kind === "correction" ? [["Reopen", "+ Request correction; - creates a linked held successor.", "reopen"], ["Follow up", "+ Add related work; - creates a separate held draft.", "followup"]] : [["Make tasks", "+ Create the displayed held drafts; - read back before separate Start.", "make"], ["Keep report", "+ Create nothing; - proposed work stays unfiled.", "keep"]];
              return { ...r4, state: "closed", revision: r4.sha + ":" + kind, resultDecision: true, repeatable: kind !== "decision" || !settled(r4), question: { parsed: true, done: kind === "correction" ? "The original task stays closed." : "Work is closed; reading does not accept it.", problem: kind === "decision" ? "What should happen to this result?" : kind === "correction" ? "Correction or additional work?" : "Create these held drafts?", resultReason: kind === "correction", resultDate: kind === "decision" && r4.kind !== "task", choices: choices.map(([label, detail, action], i4) => ({ id: i4 + 1, label, text: label, detail, action, ...kind === "correction" || action === "later" && r4.kind !== "task" ? { reason: action } : {}, local: action === "keep", recommended: i4 === (kind === "correction" ? 1 : kind === "decision" && r4.outcome !== "success" ? 1 : 0) })), evidence: [] } };
            },
            capability(r4, kind) {
              const p5 = projects.find((p6) => p6.root === r4.root), ops = [...pending.values(), ...Object.values(saved).filter((v4) => ["sending", "unknown", "partial"].includes(v4.receipt?.state)).map((v4) => v4.request).filter(Boolean)];
              const blocked = ops.some((op) => op.root === r4.root && op.n === r4.n && op.sha === r4.sha && op.action !== "read" && op.id !== saved[namespace(r4, kind)]?.op?.id);
              const reason = r4.lazy && !bodies.has(receiptKey(r4.root, r4.n, r4.sha)) ? "Reading the exact result source" : !p5 ? "Registered project unavailable" : p5.alive === false ? "Project observation stale" : p5.complete === false ? "Project observation incomplete" : pages.get(r4.root)?.unknown ? pages.get(r4.root).reason : r4.reason ? "Result unavailable: " + r4.reason : blocked ? "Another result operation needs readback" : kind === "decision" && settled(r4) ? "Report already decided" : kind === "make" && !r4.tasks?.length ? "No follow-up tasks proposed" : kind === "make" && settled(r4) ? "Report already decided" : r4.pending_decision === "make" && kind !== "make" ? "Make tasks needs readback" : "";
              return { answer: !reason, composer: true, reason };
            },
            mount(menu) {
              if (!selected) return;
              for (const slot of menu.querySelectorAll("[data-result-card]")) {
                const { body: body2, verdict, ...r4 } = selected, kind = slot.dataset.resultCard, k5 = namespace(r4, kind), row = api.cardRow(r4, kind);
                let c4 = controllers.get(k5);
                if (!c4) {
                  const el = menu.ownerDocument.createElement("div");
                  c4 = { root: r4.root, n: r4.n, sha: r4.sha, el, board: k5, ui: card.mount(el, { newId: id, capability: () => api.capability(api.rows(true).find((row2) => row2.root === r4.root && row2.n === r4.n && row2.sha === r4.sha) || r4, kind), load: () => saved[k5], save: (_b, _r, local) => {
                    const prior = saved[k5], same = prior?.op?.id === local.op?.id;
                    saved[k5] = { ...local, ...same && prior?.request ? { request: prior.request } : {}, ...same && prior?.successor ? { successor: prior.successor } : {} };
                    send({ a: "answer-local", key: k5, local: saved[k5] });
                    if (local.intent !== draft) {
                      draft = local.intent;
                      draw();
                    }
                  }, send: (_board, op) => {
                    const action = op.choice ? row.question.choices.find((c5) => c5.id === op.choice)?.action : saved[k5]?.intent;
                    if (!action) return Promise.resolve({ state: "rejected", reason: "Choose the result action first" });
                    const current = api.rows(true).find((row2) => row2.root === r4.root && row2.n === r4.n && row2.sha === r4.sha);
                    if (!current || !api.capability(current, kind).answer) return Promise.resolve({ state: "rejected", reason: "Result or project changed; nothing sent. Review its current state." });
                    const prior = saved[k5]?.request;
                    if (prior?.id !== op.id && action === "later" && r4.kind !== "task" && card.textProblem(row.question, op.text || "")) return Promise.resolve({ state: "rejected", reason: "Choose a future revisit date" });
                    const requestData = prior?.id === op.id ? prior : { id: op.id, root: r4.root, n: r4.n, sha: r4.sha, action, ...op.text !== void 0 ? { text: op.text } : {}, ...action === "later" && r4.kind !== "task" ? { until: op.text + "T12:00:00Z" } : {} };
                    if (requestData.root !== r4.root || requestData.n !== r4.n || requestData.sha !== r4.sha || requestData.action !== action || requestData.text !== op.text || action === "later" && r4.kind !== "task" && requestData.until !== op.text + "T12:00:00Z") return Promise.resolve({ state: "rejected", reason: "Saved result request differs from the displayed source; nothing sent" });
                    saved[k5] = { ...saved[k5], request: requestData };
                    return new Promise((resolve) => request(r4.root, action, { ...requestData, local_key: k5, local: saved[k5] }, resolve));
                  } }) };
                  controllers.set(k5, c4);
                }
                if (c4.el.parentNode !== slot) slot.append(c4.el);
                c4.ui.update([{ board: k5, row }]);
                if (kind === "correction" && menu.ownerDocument.defaultView.innerWidth <= 620) slot.closest("details").open = true;
              }
              const field2 = menu.querySelector("[data-c-field=result-reason]");
              if (field2 && field2 !== reasonField) {
                reasonField = field2;
                const caret = saved[namespace(selected, "correction")]?.caret;
                if (Array.isArray(caret)) field2.setSelectionRange(...caret);
              }
              readShown(selected);
            },
            reasonInput(text, caret) {
              if (!selected) return;
              const k5 = namespace(selected, "correction"), c4 = controllers.get(k5);
              if (text === null && JSON.stringify(saved[k5]?.caret) === JSON.stringify(caret)) return;
              if (c4) c4.ui.input(k5, selected.n, text, caret);
              draw();
            },
            reasonSend() {
              if (selected) {
                const k5 = namespace(selected, "correction");
                controllers.get(k5)?.ui.act(k5, selected.n, "submit");
              }
            },
            dateInput(text) {
              if (selected) {
                const k5 = namespace(selected, "decision");
                controllers.get(k5)?.ui.input(k5, selected.n, text);
                draw();
              }
            },
            section(ps) {
              api.sync(ps);
              const r4 = selected;
              if (!r4) return h5("p", null, api.count() === null ? api.reason() || "Results unknown; waiting for collection." : "No results waiting for review.");
              if (r4.lazy && !bodies.has(receiptKey(r4.root, r4.n, r4.sha))) return h5("article", { class: "report-card" }, h5("p", { role: "status" }, notice || "Reading the exact result source\u2026"), h5("button", { "data-report": "source-retry", disabled: [...pending.values()].some(active) }, "Retry source read"));
              const proposals = textPage(r4, "proposals");
              const ordinary = r4.kind === "task", nav = navigation(r4), k5 = namespace(r4, "correction"), local = saved[k5] || card.fresh(api.cardRow(r4, "correction")), context = api.cardRow(r4, "correction"), cap = api.capability(r4, "correction");
              const decision = api.cardRow(r4, "decision"), date = saved[namespace(r4, "decision")] || card.fresh(decision), dateCap = api.capability(r4, "decision"), dateLive = ["actionable", "changed"].includes(card.status(decision, date, dateCap));
              const [mark, tone, outcome2] = dashboard.outcome(r4);
              const slot = (kind) => h5("div", { "data-result-card": kind, key: namespace(r4, kind) });
              return h5(
                "article",
                { class: "report-card", "aria-label": ordinary ? "Completed result" : "Research report" },
                h5(
                  "div",
                  { class: "conversation-tools" },
                  h5("span", { class: "meta" }, (ordinary ? "Finished" : "Report") + " \xB7 " + (r4.read ? "Read" : "New") + (!ordinary ? " \xB7 " + ({ accepted: "Accepted", drop: "Dropped", made: "Drafts created", answered: "Decided", later: "Later" }[r4.decision] || "Decision open") : "")),
                  h5("a", { href: r4.url, "data-url": r4.url }, "#" + r4.n),
                  h5("button", { "data-report": "previous", disabled: nav.index <= 0 }, "Previous"),
                  h5("button", { "data-report": "next", disabled: nav.index >= nav.length - 1 }, "Next")
                ),
                ordinary && h5("p", { class: "meta" }, h5("span", { class: tone }, icons.icon(mark), outcome2), r4.merged && h5("span", null, " \xB7 " + r4.merged), time.stamp(time.seconds(r4.closed), "Task closed")),
                r4.reason && h5("p", { role: "status" }, "Result unknown: " + r4.reason),
                !ordinary && h5("p", { class: "meta" }, "Verdict: " + (r4.experiment_verdict || "unknown"), time.stamp(time.seconds(r4.closed), "Report closed")),
                ordinary ? [...["Changed", "Checks", "Open"].map((label) => h5("section", { key: label }, h5("h3", null, label), textBody(r4, field(r4.body, label), label.toLowerCase()))), r4.pr && h5("a", { href: r4.pr, "data-url": r4.pr }, "Open PR"), r4.verdict && h5("details", { "data-view-key": namespace(r4, "verdict") }, h5("summary", null, "Review and delivery"), textBody(r4, r4.verdict, "verdict"))] : r4.body ? [h5("h3", null, "Question"), textBody(r4, summary(r4.body, "Question"), "question"), h5("h3", null, "Recommendation"), textBody(r4, summary(r4.body, "Recommendation"), "recommendation")] : h5("p", null, "Report content unknown; waiting for background collection."),
                (!ordinary || saved[namespace(r4, "decision")]?.op && ["sending", "unknown", "partial"].includes(saved[namespace(r4, "decision")]?.receipt?.state)) && (settled(r4) ? h5("p", { class: "meta" }, "Decision recorded: " + r4.decision + ". Retained in Finished.") : h5("section", null, slot("decision"), date.intent === "later" && h5(
                  "div",
                  { class: "result-date" },
                  h5("label", null, "Revisit on", h5("input", { type: "date", "data-c-field": "result-date", "aria-label": "Revisit date", value: date.draft, min: new Date(Date.now() + 864e5).toISOString().slice(0, 10), disabled: !dateLive })),
                  h5("p", { class: "meta" }, "The report returns to Decide on this date."),
                  card.textProblem(decision.question, date.draft) && h5("p", { role: "status" }, card.textProblem(decision.question, date.draft)),
                  h5("div", { class: "conversation-tools" }, h5("button", { "data-report": "date-send", disabled: !dateLive || !!card.textProblem(decision.question, date.draft) }, "Send"), h5("button", { "data-report": "date-cancel", disabled: !dateLive }, "Cancel"))
                ))),
                h5("details", { class: "result-other", open: !!local.intent, "data-view-key": namespace(r4, "other") }, h5("summary", null, "Other actions"), slot("correction")),
                local.intent && composer({ id: "result-reason", field: "result-reason", label: (local.intent === "reopen" ? "Reopen" : "Follow-up") + " reason", placeholder: "What needs to change?", text: local.draft, disabled: !cap.answer || ["sending", "unknown", "partial"].includes(card.status(context, local, cap)), reason: "Creates a linked held successor. The original stays closed. Read it back before separate Start.", tools: h5("div", { class: "conversation-tools" }, h5("button", { "data-report": "reason-send", disabled: !cap.answer || !!card.textProblem(context.question, local.draft) || !["actionable", "changed"].includes(card.status(context, local, cap)) }, "Send"), h5("button", { "data-report": "reason-cancel", disabled: ["sending", "unknown", "partial"].includes(card.status(context, local, cap)) }, "Cancel")) }),
                !ordinary && Array.isArray(r4.tasks) && h5("section", null, h5("h3", null, "Proposed tasks"), r4.tasks.length ? [h5("p", { class: "meta", "data-reading-key": "report:proposals-" + proposals.start + ":0" }, "Tasks " + (proposals.start + 1) + "\u2013" + Math.min(r4.tasks.length, proposals.start + 64) + " / " + r4.tasks.length), r4.tasks.slice(proposals.start, proposals.start + 64).map((t4, i4) => {
                  const index = proposals.start + i4, page = textPage(r4, "proposal" + index);
                  return h5("details", { key: index, open: page.open, "data-view-key": namespace(r4, "proposal" + index) }, h5("summary", { onClick: (e4) => {
                    e4.preventDefault();
                    page.open = !page.open;
                    draw();
                  } }, index + 1 + ". " + t4.title), page.open && textBody(r4, [t4.goal, ...(t4.acceptance || []).map((a4) => "- " + a4), ...t4.scope || []].join("\n\n"), "proposal" + index));
                }), h5("div", { class: "conversation-tools" }, proposals.start > 0 && h5("button", { onClick: () => {
                  proposals.start = Math.max(0, proposals.start - 64);
                  reading("report:proposals-" + proposals.start + ":0", r4);
                  draw();
                } }, "Previous proposals"), proposals.start + 64 < r4.tasks.length && h5("button", { onClick: () => {
                  proposals.start += 64;
                  reading("report:proposals-" + proposals.start + ":0", r4);
                  draw();
                } }, "Next proposals"))] : h5("p", null, "No follow-up tasks proposed."), r4.tasks.length > 0 && h5("p", { class: "meta" }, "Make tasks creates held drafts linked to this report. Start them separately."), r4.pending_decision === "make" && h5("p", { role: "status" }, "Make tasks is pending. Retry Make tasks to reconcile the original claims."), (r4.tasks.length > 0 || r4.pending_decision === "make") && !settled(r4) && slot("make")),
                !ordinary && r4.body && h5("details", { "data-view-key": namespace(r4, "full"), open: textPage(r4, "full").open, onToggle: (e4) => {
                  const page = textPage(r4, "full");
                  if (page.open !== e4.currentTarget.open) {
                    page.open = e4.currentTarget.open;
                    draw();
                  }
                } }, h5("summary", { "data-reading-key": "report:full", onClick: (e4) => {
                  e4.preventDefault();
                  const page = textPage(r4, "full");
                  page.open = !page.open;
                  e4.currentTarget.parentElement.open = page.open;
                  draw();
                } }, "Full report"), textPage(r4, "full").open && textBody(r4, r4.body, "full")),
                transcript?.(r4),
                notice && h5("p", { role: "status" }, notice),
                !!(notice || successor || r4.tasks_made?.length) && ![...pending.values()].some((op) => active(op) && op.action !== "read") && h5(
                  "div",
                  { class: "conversation-tools result-next" },
                  successor && h5("a", { href: successor.url, "data-url": successor.url }, "1. Read held draft #" + successor.n + " \u2605"),
                  !!r4.tasks_made?.length && h5("details", { "data-view-key": namespace(r4, "drafts") }, h5("summary", null, "1. Read held drafts \u2605"), r4.tasks_made.map((n3) => h5("a", { href: r4.url?.replace(/\d+$/, String(n3)), "data-url": r4.url?.replace(/\d+$/, String(n3)), key: n3 }, "Read held draft #" + n3))),
                  h5("button", { "data-report": "unresolved" }, successor || r4.tasks_made?.length ? "2. Next unresolved item" : "1. Next unresolved item \u2605"),
                  h5("button", { "data-report": "stay" }, (successor || r4.tasks_made?.length ? "3" : "2") + ". Stay here")
                )
              );
            }
          });
          return api;
        },
        // Keep untrusted Markdown as text/vnodes; evidence URLs are the only active content. No raw HTML.
        body: function(text, source = "body", nativeAt) {
          if (nativeAt === void 0) text = text.replace(/```taskq-tasks\n[\s\S]*?```/g, "").replace(/(^|\n)#{1,6}[ \t]+Proposed tasks[ \t]*(?=\n|$)/gi, "$1");
          const lines = text.split("\n"), starts = [];
          let offset = 0;
          for (const line of lines) {
            starts.push(offset);
            offset += line.length + 1;
          }
          const blocks = [], props = (at) => nativeAt === void 0 ? source === null ? {} : { "data-reading-key": "report:" + source + ":" + blocks.length } : { "data-native-part": nativeAt + at, ...source ? { "data-reading-key": "report:" + source + ":part:" + (nativeAt + at) } : {} };
          const plain = (text2, at) => {
            const nodes = [];
            for (let n3 = 0; n3 < text2.length; ) {
              let end = Math.min(text2.length, n3 + 1024);
              if (end < text2.length && /[\uD800-\uDBFF]/.test(text2[end - 1])) end++;
              nodes.push(nativeAt === void 0 ? text2.slice(n3, end) : h5("span", { key: at + n3, ...props(at + n3) }, text2.slice(n3, end)));
              n3 = end;
            }
            return nodes;
          };
          const inline = (text2, at) => {
            const nodes = [];
            let end = 0;
            for (const m5 of text2.matchAll(/`(?<code>[^`\n]+)`|\*\*(?<strong>[^*\n]+)\*\*|\*(?<em>[^*\n]+)\*|_(?<italic>[^_\n]+)_|\[(?<label>[^\]\n]+)\]\((?<url>https?:\/\/[^\s)]+)\)|https?:\/\/[^\s<>\]\)]+/g)) {
              nodes.push(...plain(text2.slice(end, m5.index), at + end));
              const g5 = m5.groups, value = g5.code ?? g5.strong ?? g5.em ?? g5.italic ?? g5.label ?? m5[0], tag = g5.code !== void 0 ? "code" : g5.strong !== void 0 ? "strong" : g5.em !== void 0 || g5.italic !== void 0 ? "em" : "a";
              nodes.push(h5(tag, { key: at + m5.index, ...tag === "a" ? { href: g5.url || m5[0], "data-url": g5.url || m5[0] } : {} }, plain(value, at + m5.index + (g5.strong !== void 0 ? 2 : tag === "a" && !g5.label ? 0 : 1))));
              end = m5.index + m5[0].length;
            }
            return [...nodes, ...plain(text2.slice(end), at + end)];
          };
          const code = (text2, language, at) => {
            const name = language.toLowerCase().split(/\s/)[0], grammar = Object.hasOwn(Prism2.languages, name) && Prism2.languages[name];
            if (!grammar || text2.length > 8192) return plain(text2, at);
            const key = name + "\n" + text2;
            let tokens = syntaxCache.get(key);
            try {
              if (!tokens) {
                tokens = Prism2.tokenize(text2, grammar);
                syntaxCache.set(key, tokens);
                while (syntaxCache.size > 16) syntaxCache.delete(syntaxCache.keys().next().value);
              }
            } catch {
              return plain(text2, at);
            }
            let cursor = at;
            const render = (token) => {
              if (typeof token === "string") {
                const nodes = plain(token, cursor);
                cursor += token.length;
                return nodes;
              }
              if (Array.isArray(token)) return token.map(render);
              return h5("span", { class: "syntax-" + token.type }, render(token.content));
            };
            return render(tokens);
          };
          const list = (line) => /^\s*(?:([-*+])|(\d+)[.)])\s+(.+)$/.exec(line);
          for (let i4 = 0; i4 < lines.length; ) {
            if (!lines[i4].trim()) {
              i4++;
              continue;
            }
            const start = i4, at = starts[i4], fence = /^\s*(`{3,}|~{3,})([^\n]*)$/.exec(lines[i4]);
            let node;
            if (lines[i4].trim() === "<oai-mem-citation>" && lines.slice(i4 + 1).some((l5) => l5.trim() === "</oai-mem-citation>")) {
              i4++;
              while (i4 < lines.length && lines[i4].trim() !== "</oai-mem-citation>") i4++;
              i4++;
              node = h5("details", { "data-view-key": "metadata:" + source + ":" + at }, h5("summary", { class: "source-toggle" }, "Provider citation metadata"), h5("pre", null, plain(lines.slice(start, i4).join("\n"), at)));
            } else if (fence) {
              i4++;
              const begin = i4;
              while (i4 < lines.length && !lines[i4].trim().startsWith(fence[1])) i4++;
              node = h5("pre", null, h5("code", { "aria-label": (fence[2].trim() || "Text") + " code" }, code(lines.slice(begin, i4).join("\n"), fence[2].trim(), starts[begin] ?? text.length)));
              if (i4 < lines.length) i4++;
            } else if (/^#{1,6}\s/.test(lines[i4])) {
              const heading = /^#{1,6}\s+/.exec(lines[i4])[0];
              node = h5("h3", null, inline(lines[i4].slice(heading.length), at + heading.length));
              i4++;
            } else if (list(lines[i4])) {
              const ordered = !!list(lines[i4])[2], items = [];
              while (i4 < lines.length) {
                const m5 = list(lines[i4]);
                if (!m5 || !!m5[2] !== ordered) break;
                items.push(h5("li", { key: i4, ...ordered ? { value: Number(m5[2]) } : {}, ...props(starts[i4]) }, inline(m5[3], starts[i4] + lines[i4].indexOf(m5[3]))));
                i4++;
              }
              node = h5(ordered ? "ol" : "ul", null, items);
            } else {
              i4++;
              while (i4 < lines.length && lines[i4].trim() && !/^\s*(`{3,}|~{3,})|^#{1,6}\s/.test(lines[i4]) && !list(lines[i4]) && lines[i4].trim() !== "<oai-mem-citation>") i4++;
              node = h5("p", null, inline(lines.slice(start, i4).join("\n"), at));
            }
            Object.assign(node.props, { ...props(at), style: { whiteSpace: "pre-wrap", overflowWrap: "anywhere" } });
            node.key = at;
            blocks.push(node);
          }
          return blocks;
        }
      });
      module.exports = { create, body };
    }
  });

  // src/notification-panel.js
  var require_notification_panel = __commonJS({
    "src/notification-panel.js"(exports, module) {
      "use strict";
      var time = require_relative_time();
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var { icon } = require_ui_icons2();
      var { execution } = require_ui_runtime();
      var kinds = { ask: "Question", close: "Finished", failed: "Failed", gone: "Attempt ended", blocked: "Blocked" };
      var api = execution.module("notificationPanel", {
        label: function(kind) {
          return kinds[kind] || "Event";
        },
        // M14: result SHA owns result reads; local event markers never become queue/decision state.
        rows: function(projects = [], results = [], marks = {}) {
          const seen = /* @__PURE__ */ new Set(), rows = [], ids = /* @__PURE__ */ new Set(), byRoot = /* @__PURE__ */ new Map(), byTask = /* @__PURE__ */ new Map();
          for (const r4 of results) {
            if (!byRoot.has(r4.root)) byRoot.set(r4.root, []);
            byRoot.get(r4.root).push(r4);
            byTask.set(r4.root + "#" + r4.n, r4);
          }
          const closed = new Set(byTask.keys());
          for (const p5 of projects) {
            const board = p5.issues || p5.root;
            if (seen.has(board)) continue;
            seen.add(board);
            const tasks = new Map((p5.tasks || []).map((t4) => [t4.n, t4]));
            const add = (r4) => {
              r4.id = JSON.stringify([board, r4.source, r4.n, r4.source === "result" ? r4.sha : [r4.at, r4.kind, r4.event]]);
              r4.read = r4.source === "result" ? !!r4.read : marks[r4.id] === "read" || marks[r4.id] === "cleared";
              if (marks[r4.id] !== "cleared" && !ids.has(r4.id)) {
                ids.add(r4.id);
                rows.push(r4);
              }
            };
            for (const r4 of byRoot.get(p5.root) || []) add({ ...r4, source: "result", resultKind: r4.kind, kind: "close", at: r4.closed, project: p5.name });
            for (const e4 of p5.events || []) {
              if (!kinds[e4[1]] || e4[1] === "close" && closed.has(p5.root + "#" + e4[3])) continue;
              const task = tasks.get(e4[3]), result = byTask.get(p5.root + "#" + e4[3]);
              add({ source: "event", root: p5.root, project: p5.name, n: e4[3], at: e4[0], kind: e4[1], event: e4[2], title: task?.title || result?.title || "Task #" + e4[3] });
            }
          }
          return rows.sort((a4, b5) => b5.at - a4.at || b5.n - a4.n || a4.id.localeCompare(b5.id));
        },
        groups: function(rows) {
          const groups = [], active = /* @__PURE__ */ new Map();
          for (const row of rows) {
            const series = /^(.*?)\b([1-9]\d*)\/([1-9]\d*)\b/.exec(row.title), ordinal = series && Number(series[2]);
            const key = series && ordinal <= Number(series[3]) ? JSON.stringify([row.root, row.kind, series[1], series[3]]) : null, prior = active.get(key);
            if (key && prior && prior.rows.at(-1).at - row.at <= 600 && !prior.ordinals.has(ordinal)) {
              prior.rows.push(row);
              prior.ordinals.add(ordinal);
            } else {
              const group = { title: series?.[1].trim(), rows: [row], ordinals: /* @__PURE__ */ new Set([ordinal]) };
              groups.push(group);
              if (key) active.set(key, group);
            }
          }
          return groups;
        },
        // #2193: unread events of one task and kind (Question, Attempt ended, Blocked) are one row with a count, wherever they
        // fall in the list; read events keep merging only with an adjacent event of the same attempt.
        collapse: function(rows = []) {
          const groups = [], fresh = /* @__PURE__ */ new Map();
          for (const r4 of rows) {
            if (r4.source === "event" && !r4.read) {
              const key = JSON.stringify([r4.root, r4.n, r4.kind]), g5 = fresh.get(key);
              if (g5) g5.rows.push(r4);
              else {
                const group = { rows: [r4] };
                fresh.set(key, group);
                groups.push(group);
              }
              continue;
            }
            const last = groups.at(-1), same = last && last.rows[0].read && last.rows[0].kind === r4.kind && last.rows[0].n === r4.n && last.rows[0].root === r4.root && last.rows[0].event?.attempt === r4.event?.attempt;
            if (same) last.rows.push(r4);
            else groups.push({ rows: [r4] });
          }
          return groups;
        },
        // M14: intersecting sets of canonical source identities, independent of event/page counts.
        attention: function(projects = [], results = [], messages = {}, marks = {}, owners = [], sidebar = {}) {
          const fresh = /* @__PURE__ */ new Set(), needs = /* @__PURE__ */ new Set(), owned = new Set(owners.map((c4) => c4.id)), links = /* @__PURE__ */ new Map();
          for (const p5 of projects) for (const t4 of p5.tasks || []) if (owned.has(t4.conversation?.id)) links.set((p5.issues || p5.root) + "/" + t4.n, t4.conversation.id);
          for (const r4 of results) {
            const p5 = projects.find((p6) => p6.root === r4.root);
            if (owned.has(r4.conversation)) links.set((p5?.issues || r4.root) + "/" + r4.n, r4.conversation);
          }
          const key = (board, n3) => links.has(board + "/" + n3) ? "owner:" + links.get(board + "/" + n3) : "task:" + board + "/" + n3;
          const archived = new Set(owners.filter((c4) => c4.archived).map((c4) => "owner:" + c4.id));
          for (const id of Object.keys(sidebar.archived || {})) archived.add(id);
          for (const r4 of messages.entries || []) if (r4.archived && r4.kind === "question" && !links.has(r4.board + "/" + r4.task)) archived.add(key(r4.board, r4.task));
          const add = (set, id) => {
            if (!archived.has(id)) set.add(id);
          };
          for (const r4 of api.rows(projects, results, marks)) if (!r4.read) {
            const p5 = projects.find((p6) => p6.root === r4.root);
            add(fresh, key(p5?.issues || r4.root, r4.n));
          }
          for (const r4 of messages.entries || []) if (!r4.archived && marks[JSON.stringify(["message", r4.key])] !== "cleared") {
            const id = r4.kind === "question" ? key(r4.board, r4.task) : "owner:" + r4.id;
            if (!r4.read) add(fresh, id);
          }
          for (const p5 of projects) for (const t4 of p5.tasks || []) if (t4.state === "ask") add(needs, key(p5.issues || p5.root, t4.n));
          for (const r4 of results) if (r4.kind !== "task" && r4.visible !== false && !r4.pending_decision && (!r4.decision || r4.decision === "none" || r4.decision === "later" && !(Date.parse(r4.later_until) > Date.now()))) {
            const p5 = projects.find((p6) => p6.root === r4.root);
            add(needs, key(p5?.issues || r4.root, r4.n));
          }
          return { fresh, needs, key };
        },
        create: function({ view, reports, send, draw, messages }) {
          let notice = "";
          const historyViews = /* @__PURE__ */ new Map();
          const model = execution.module("notificationPanel.view", {
            rows() {
              return api.rows(view.S.projects || [], reports.rows(), view.S.notify?.inbox || {});
            },
            reasons() {
              return [.../* @__PURE__ */ new Set([...reports.count() === null ? ["Results incomplete; check result collection."] : [], ...(view.S.projects || []).flatMap((p5) => p5.events_error || p5.alive === false ? [p5.name + ": " + (p5.events_error || "project observation stale")] : [])])];
            },
            messages() {
              return (view.S.messages?.entries || []).filter((r4) => !r4.archived && view.S.notify?.inbox?.[JSON.stringify(["message", r4.key])] !== "cleared");
            },
            taskMessages(board, n3) {
              return model.messages().filter((r4) => r4.kind === "question" && r4.board === board && r4.task === n3);
            },
            attention(sidebar = view.S.conversations?.sidebar || {}) {
              const sets = api.attention(view.S.projects || [], reports.rows(), view.S.messages, view.S.notify?.inbox || {}, view.S.conversations?.items || [], sidebar), reasons = [...model.reasons(), ...view.S.messages?.complete !== true ? ["Message collection incomplete"] : [], ...view.S.conversations?.complete !== true ? ["Conversation collection incomplete"] : []];
              return { ...sets, reasons };
            },
            label(a4 = model.attention()) {
              return "Conversations \xB7 " + a4.fresh.size + (a4.reasons.length ? " loaded New" : " New") + " \xB7 " + a4.needs.size + (a4.reasons.length ? " loaded Needs me" : " Needs me") + (a4.reasons.length ? " \xB7 Partial: " + a4.reasons.join("; ") : "");
            },
            entry() {
              const a4 = model.attention(), label = model.label(a4), badge = (kind, n3, mark) => h5("span", { class: "communication-count " + kind, title: kind + ": " + n3, "aria-label": kind + ": " + n3 }, icon(mark), h5("span", { "aria-hidden": "true" }, a4.reasons.length ? n3 ? "\u2265" + (n3 > 99 ? "99" : n3) : "?" : n3 > 99 ? "99+" : String(n3)));
              return h5("button", { class: "conversations-entry", type: "button", "data-a": "workspace", "data-page": "conversations", "data-thread": "centre", title: label, "aria-label": label }, icon("messages-square"), badge("New", a4.fresh.size, "inbox"), badge("Needs me", a4.needs.size, "message-circle-question"), a4.reasons.length > 0 && h5("span", { class: "notification-warning" }, icon("circle-alert")));
            },
            bell() {
              return model.entry();
            },
            save(action, rows) {
              if (rows.length) send({ a: "notification-state", action, ids: rows.map((r4) => r4.id) });
            },
            read(rows, entries = []) {
              if (entries.length) messages?.("read", entries.filter((r4) => !r4.read));
              model.save("read", rows.filter((r4) => r4.source !== "result" && !r4.read));
              reports.markRead(rows.filter((r4) => r4.source === "result" && !r4.read));
            },
            receive(r4) {
              notice = r4.reason || "";
              draw();
            },
            click(b5) {
              const action = b5.dataset.notification;
              if (!action) return false;
              const rows = model.rows();
              if (action === "read") model.read(rows, model.messages());
              else if (action === "clear") model.save("clear", [...rows.filter((r4) => r4.read), ...model.messages().filter((r4) => r4.read).map((r4) => ({ id: JSON.stringify(["message", r4.key]) }))]);
              return true;
            },
            tools() {
              const rows = [...model.rows(), ...model.messages()];
              return h5("div", { class: "news-tools" }, h5("div", { class: "conversation-tools" }, h5("button", { "data-notification": "read", disabled: !!view.messageBusy || !rows.some((r4) => !r4.read) }, "Mark all read"), h5("button", { "data-notification": "clear", disabled: !rows.some((r4) => r4.read) }, "Clear read")), notice && h5("p", { role: "alert" }, notice), reports.readFailure() && h5("p", { role: "status" }, reports.readFailure()), model.reasons().map((reason) => h5("p", { role: "status", class: "meta", key: reason }, reason)));
            },
            history(rows = [], extra, key = rows[0]?.root + "#" + rows[0]?.n, entries = []) {
              if (!rows.length && !extra && !entries.some((r4) => !r4.read)) return null;
              const state = historyViews.get(key) || { open: false, start: 0 };
              historyViews.delete(key);
              historyViews.set(key, state);
              while (historyViews.size > 24) historyViews.delete(historyViews.keys().next().value);
              const all = api.collapse(rows), groups = all.slice(state.start, state.start + 64);
              return h5("details", { class: "thread-events", "data-view-key": "history:" + key, open: state.open, onToggle: (e4) => {
                if (state.open !== e4.currentTarget.open) {
                  state.open = e4.currentTarget.open;
                  draw();
                }
              } }, h5("summary", { onClick: (e4) => {
                e4.preventDefault();
                state.open = !state.open;
                e4.currentTarget.parentElement.open = state.open;
                draw();
              } }, "Earlier history" + (rows.length ? " \xB7 " + rows.length + " events" : "")), state.open && [
                (rows.some((r4) => !r4.read) || entries.some((r4) => !r4.read)) && h5("button", { "data-notification": "thread-read", disabled: !!view.messageBusy, onClick: () => model.read(rows, entries) }, "Mark thread read"),
                groups.map((g5) => {
                  const r4 = g5.rows[0], text = (r4.read ? "Read" : "New") + " \xB7 " + (r4.source === "result" && r4.resultKind !== "task" ? "Report ready" : kinds[r4.kind]) + " \xB7 " + r4.project + " \xB7 #" + r4.n;
                  return h5("div", { key: r4.id, "data-event": r4.id, class: "meta" }, h5("p", null, text, g5.rows.length > 1 ? " \xD7" + g5.rows.length : "", time.stamp(time.seconds(r4.at), "Event recorded"), !r4.read && h5("button", { "data-notification": "group-read", disabled: !!view.messageBusy, onClick: () => model.read(g5.rows) }, "Mark read")), g5.rows.length > 1 ? h5("details", null, h5("summary", null, "Source events"), g5.rows.map((r5) => h5("p", { key: r5.id, "data-event": r5.id }, r5.event || r5.kind, time.stamp(time.seconds(r5.at), "Event recorded")))) : null);
                }),
                h5("div", { class: "event-tools" }, state.start > 0 && h5("button", { onClick: () => {
                  state.start = Math.max(0, state.start - 64);
                  draw();
                } }, "Newer events"), state.start + 64 < all.length && h5("button", { onClick: () => {
                  state.start += 64;
                  draw();
                } }, "Older events")),
                typeof extra === "function" ? extra() : extra
              ]);
            }
          });
          return model;
        }
      });
      module.exports = api;
    }
  });

  // src/tray-view-state.js
  var require_tray_view_state = __commonJS({
    "src/tray-view-state.js"(exports, module) {
      "use strict";
      var { execution } = require_ui_runtime();
      var { normalize, create } = execution.module("tray.viewState", {
        normalize: function(saved, snapshot, routes, root = "home") {
          const roots = new Set((snapshot.projects || []).map((p5) => p5.root)), states = new Set(Object.keys(require_project_blocks().marks));
          const { entry } = execution.module("tray.viewState.normalize", {
            entry: function(value) {
              const v4 = value && typeof value === "object" ? value : {};
              if (v4.page === "messages" && root === "conversations") return entry({ ...v4, page: root, section: "inbox" });
              let page = typeof v4.page === "string" ? v4.page : root;
              const project = roots.has(page) ? page : null;
              if (!project && !Object.hasOwn(routes, page)) page = v4.project || page.startsWith("/") ? Object.hasOwn(routes, "projects") ? "projects" : root : page.startsWith("settings/") && Object.hasOwn(routes, "settings") ? "settings" : Object.hasOwn(routes, v4.parent) ? v4.parent : root;
              const valid = page === v4.page, open = {};
              const work = valid && v4.working && typeof v4.working === "object" && !Array.isArray(v4.working) ? Object.fromEntries(Object.entries(v4.working).filter(([root2, on]) => roots.has(root2) && on === true)) : null;
              if (valid && v4.open && typeof v4.open === "object") {
                for (const [k5, b5] of Object.entries(v4.open)) if (typeof b5 === "boolean" && k5.length <= 240) open[k5] = b5;
              }
              return {
                page,
                operation: root !== "home" && v4.operation && typeof v4.operation.operation === "string" ? { operation: v4.operation.operation, root: typeof v4.operation.root === "string" ? v4.operation.root : null } : null,
                project: roots.has(page) ? page : null,
                parent: routes[page]?.parent || root,
                taskFilter: valid && Array.isArray(v4.taskFilter) ? [...new Set(v4.taskFilter.filter((s4) => states.has(s4)))] : null,
                working: work && (Object.keys(work).length || !Object.keys(v4.working).length) ? work : null,
                aiRoot: roots.has(v4.aiRoot) || root !== "home" && typeof v4.aiRoot === "string" ? v4.aiRoot : null,
                answer: root !== "home" && v4.answer && typeof v4.answer.board === "string" && Number.isSafeInteger(v4.answer.task) && v4.answer.task > 0 ? { board: v4.answer.board, task: v4.answer.task, revision: v4.answer.revision, root: v4.answer.root } : null,
                ...root !== "home" ? { messageFilter: ["all", "pending", "archived"].includes(v4.messageFilter) ? v4.messageFilter : "all", message: typeof v4.message === "string" && /^[a-f0-9]{64}$/.test(v4.message) ? v4.message : null, section: v4.section === "inbox" ? "inbox" : null } : {},
                scroll: valid && Number.isFinite(v4.scroll) && v4.scroll >= 0 ? v4.scroll : 0,
                open,
                focus: valid && v4.focus && /^(BUTTON|A|SUMMARY|INPUT|SELECT|TEXTAREA|H1|DIV)$/.test(v4.focus.tag) && typeof v4.focus.key === "string" && v4.focus.key.length <= 2048 && Number.isInteger(v4.focus.index) && v4.focus.index >= 0 && v4.focus.index < 4096 ? v4.focus : null,
                unavailable: !valid && typeof v4.page === "string" ? "Requested destination is no longer available." : typeof v4.unavailable === "string" ? v4.unavailable.slice(0, 160) : null
              };
            }
          });
          if (saved?.schema !== 1) return { schema: 1, current: entry(null), trail: [], ...root === "home" ? { dashboard: require_task_views().normalize(null) } : {} };
          const state = { schema: 1, ...root === "home" ? { dashboard: require_task_views().normalize(saved.dashboard, saved.current) } : {}, current: entry(saved.current), trail: (Array.isArray(saved.trail) ? saved.trail : []).slice(-32).filter((v4) => v4 && entry(v4).page === v4.page).map(entry) };
          if (root === "conversations") while (state.trail.length && new TextEncoder().encode(JSON.stringify(state)).length > 65536) state.trail.shift();
          return state;
        },
        create: function(view, { menu, document: document2, send, defer, cancel, routes, trail, root = "home" }) {
          let hydrated = false, timer = null, last = "", restoring = false;
          const api = execution.module("tray.viewState.instance", {
            groups: function() {
              return Object.fromEntries([...menu.querySelectorAll("details[data-view-key]:not(.shell-overflow)")].map((d5) => [d5.dataset.viewKey, d5.open]));
            },
            capture: function() {
              const active = document2.activeElement?.closest?.(".shell-overflow")?.querySelector("summary") || document2.activeElement, key = active && JSON.stringify(active.dataset), focus = active && menu.contains?.(active) ? { tag: active.tagName, key, index: [...menu.querySelectorAll(active.tagName)].filter((e4) => JSON.stringify(e4.dataset) === key).indexOf(active) } : null;
              return { page: view.page, operation: view.operation, project: view.S?.projects?.some((p5) => p5.root === view.page) ? view.page : null, parent: routes[view.page]?.parent || root, taskFilter: view.taskFilter, working: view.working, aiRoot: view.aiRoot, answer: view.answer, messageFilter: view.messageFilter, message: view.message, section: view.section, scroll: (menu.querySelector(".content") || document2.scrollingElement)?.scrollTop || 0, open: root === "conversations" ? { ...view.disclosures, ...api.groups() } : api.groups(), focus, unavailable: view.unavailable };
            },
            apply: function(v4) {
              view.page = v4.page;
              view.operation = v4.operation;
              view.answer = v4.answer;
              view.message = v4.message;
              view.messageFilter = v4.messageFilter;
              view.section = v4.section;
              view.taskFilter = v4.taskFilter;
              view.working = v4.working;
              view.aiRoot = v4.aiRoot;
              view.disclosures = v4.open;
              view.restoreScroll = v4.scroll;
              view.restoreFocus = v4.focus;
              view.unavailable = v4.unavailable;
            },
            focus: function(saved) {
              const target = saved ? [...menu.querySelectorAll(saved.tag)].filter((e4) => JSON.stringify(e4.dataset) === saved.key)[saved.index] : null;
              (target?.checkVisibility?.() && !target.disabled ? target : menu.querySelector("h1"))?.focus({ preventScroll: true });
            },
            hydrate: function(snapshot) {
              if (snapshot.page !== void 0) {
                hydrated = true;
                return;
              }
              if (!hydrated) {
                hydrated = true;
                const state = normalize(snapshot.navigation, snapshot, routes, root);
                api.apply(state.current);
                if (root === "home") {
                  view.dashboard = state.dashboard;
                  if (!snapshot.navigation?.dashboard && snapshot.navigation?.current?.taskFilter?.length) {
                    const p5 = snapshot.projects.find((p6) => p6.root === state.current.page);
                    if (p5) require_task_views().current(view.dashboard).working.projects = [require_task_views().identity(p5)];
                  }
                }
                trail.push(...state.trail);
              } else {
                const current = api.capture(), state = normalize({ schema: 1, current, trail }, snapshot, routes, root);
                if (state.current.page !== view.page || JSON.stringify(state.current.working) !== JSON.stringify(view.working) || JSON.stringify(state.current.taskFilter) !== JSON.stringify(view.taskFilter)) api.apply(state.current);
                view.aiRoot = state.current.aiRoot;
                for (let i4 = trail.length - 1; i4 >= 0; i4--) {
                  const clean = normalize({ schema: 1, current: trail[i4] }, snapshot, routes, root).current;
                  if (clean.page !== trail[i4].page) trail.splice(i4, 1);
                  else Object.assign(trail[i4], clean);
                }
              }
            },
            restore: function(open) {
              restoring = true;
              for (const d5 of menu.querySelectorAll("details[data-view-key]")) if (typeof open?.[d5.dataset.viewKey] === "boolean" && d5.open !== open[d5.dataset.viewKey]) d5.open = open[d5.dataset.viewKey];
              restoring = false;
            },
            flush: function() {
              cancel(timer);
              timer = null;
              if (!hydrated || view.S?.page !== void 0) return;
              if (root === "home" && view.dashboard && view.page === "home") require_task_views().current(view.dashboard).scroll = (menu.querySelector(".content") || document2.scrollingElement)?.scrollTop || 0;
              const navigation = normalize({ schema: 1, current: api.capture(), trail, ...root === "home" ? { dashboard: view.dashboard } : {} }, view.S, routes, root), encoder = new TextEncoder();
              let text = JSON.stringify(navigation);
              if (root === "conversations" && encoder.encode(text).length > 65536) {
                const open = navigation.current.open, visible = api.groups();
                navigation.current.open = Object.fromEntries(Object.entries(open).filter(([key]) => Object.hasOwn(visible, key)));
                let bytes = encoder.encode(JSON.stringify(navigation)).length, has = Object.keys(navigation.current.open).length > 0;
                for (const [key, value] of Object.entries(open).reverse()) {
                  if (Object.hasOwn(visible, key)) continue;
                  const added = encoder.encode(JSON.stringify(key) + ":" + value).length + (has ? 1 : 0);
                  if (bytes + added <= 65536) {
                    navigation.current.open[key] = value;
                    bytes += added;
                    has = true;
                  }
                }
                navigation.current.open = Object.fromEntries(Object.entries(open).filter(([key]) => Object.hasOwn(navigation.current.open, key)));
                text = JSON.stringify(navigation);
              }
              if (encoder.encode(text).length > 65536) {
                view.navigationError = "Workspace view state exceeds 64 KiB; current context kept in this window, previous saved state retained.";
                return;
              }
              if (text !== last) {
                last = text;
                send({ a: "set", navigation });
              }
            },
            schedule: function() {
              if (restoring) return;
              cancel(timer);
              timer = defer(api.flush, 100);
            }
          });
          return api;
        }
      });
      module.exports = { normalize, create };
    }
  });

  // src/workspace-answers.js
  var require_workspace_answers = __commonJS({
    "src/workspace-answers.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var { execution } = ui;
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var replyFiles = require_reply_files();
      var time = require_relative_time();
      var { create } = execution.module("workspace.answers", {
        create: function(view, { menu, document: document2, send, draw, defer = ui.defer, cancel = ui.cancel, composer = false, changed, steer, read = (p5, t4) => t4, historyText }) {
          const answerRequests = /* @__PURE__ */ new Map(), answerCards = /* @__PURE__ */ new Map(), answerFeedback = /* @__PURE__ */ new Map(), saved = {}, steers = {};
          const api = execution.module("workspace.answers.instance", {
            project: function(board, root = view.answer?.board === board ? view.answer.root : null) {
              const matches = view.S?.projects.filter((p5) => p5.issues === board && (!root || p5.root === root)) || [];
              return matches.length === 1 ? matches[0] : null;
            },
            answerCapability: function(board, row) {
              const p5 = api.project(board);
              return { manage: true, files: true, answer: !!p5 && p5.alive && p5.complete === true, reason: "Project status is unavailable or stale; check the board", composer, paged: composer && (row?.question?.text || row?.card || "").length > 32768 };
            },
            // #1662 A3: the workspace's shared composer reads and drives the mounted card; the card keeps draft, op and receipt
            answerCompose: function(board, row) {
              return component().compose(row, saved[board + "#" + row.n] || component().fresh(row), answerCapability(board, row));
            },
            answerInput: function(board, n3, text, caret) {
              if (text == null && JSON.stringify(saved[board + "#" + n3]?.caret) === JSON.stringify(caret)) return;
              answerCards.get(board + "/" + n3)?.ui.input(board, n3, text, caret);
            },
            answerAct: function(board, n3, type) {
              answerCards.get(board + "/" + n3)?.ui.act(board, n3, type);
            },
            answerSetFiles: function(board, n3, files) {
              const row = { n: n3 }, prior = saved[board + "#" + n3];
              if (!prior) return;
              if (!answerCards.get(board + "/" + n3)?.ui.files(board, n3, files)) save(board, row, { ...prior, files });
              draw();
            },
            answerFiles: function(board, n3, files) {
              if (!saved[board + "#" + n3]) {
                const row = api.project(board)?.tasks.find((t4) => t4.n === n3);
                if (!row) return;
                save(board, row, component().fresh(row));
              }
              replyFiles.read(files, (file) => {
                const current = saved[board + "#" + n3]?.files || [];
                if (file.pending || current.some((f4) => f4.id === file.id)) api.answerSetFiles(board, n3, [...current.filter((f4) => f4.id !== file.id), file]);
              });
            },
            answerFileReceipt: function(r4) {
              replyFiles.settle(r4.id);
              if (r4.state !== "accepted") replyFiles.refuse(r4.kind);
              for (const [key, local] of Object.entries(saved)) if (local.files?.some((f4) => f4.id === r4.id)) {
                const at = key.lastIndexOf("#");
                api.answerSetFiles(key.slice(0, at), Number(key.slice(at + 1)), local.files.map((f4) => f4.id === r4.id ? r4.state === "accepted" ? r4.file : { ...f4, pending: false, error: r4.reason } : f4));
              }
            },
            fileControls: function(board, n3, disabled) {
              return replyFiles.controls(saved[board + "#" + n3]?.files || [], disabled, (files) => api.answerFiles(board, n3, files), (id) => api.answerSetFiles(board, n3, (saved[board + "#" + n3]?.files || []).filter((f4) => f4.id !== id)));
            },
            fileEvents: function(board, n3, disabled) {
              return replyFiles.events((files) => api.answerFiles(board, n3, files), disabled);
            },
            answerReceipt: function(r4) {
              const key = r4.root + "#" + r4.id, request = answerRequests.get(key);
              const previous = answerFeedback.get(key);
              if (!request && previous?.state !== "unknown") return;
              const item = request || previous;
              api.answerRefusal(r4.state);
              answerFeedback.set(key, { board: item.board, op: item.op, about: item.about, state: r4.state, reason: r4.reason, next: r4.next, answered: r4.answered, ...r4.note ? { note: true } : {} });
              const local = saved[item.board + "#" + item.op.task];
              if (local?.op?.id === r4.id) save(item.board, { n: item.op.task }, { ...local, receipt: { state: r4.state, reason: r4.reason, next: r4.next, answered: r4.answered, ...r4.note ? { note: true } : {} }, ...r4.state === "accepted" && (item.op.text || item.op.files?.length) ? { draft: "", files: [] } : {} });
              if (item.op.action === "steer" && r4.state === "accepted") steers[item.board + "#" + item.op.task] = "";
              if (r4.state === "accepted") {
                const answered = [...answerFeedback].filter(([, x5]) => x5.state === "accepted");
                if (answered.length > 20) answerFeedback.delete(answered[0][0]);
                view.toast = key;
                defer(() => {
                  if (view.toast === key) {
                    view.toast = null;
                    draw();
                  }
                }, 6e3);
              }
              if (request) {
                cancel(request.timer);
                answerRequests.delete(key);
                request.resolve({ state: r4.state, reason: r4.reason, next: r4.next, ...r4.note ? { note: true } : {} });
              } else answerCards.get(item.board + "/" + item.op.task)?.ui.receiveReceipt(item.board, r4.id, r4);
              draw();
            },
            answerRefusal: function(state) {
              switch (state) {
                case "rejected":
                  execution.action("workspace.answer.refused.rejected", () => {
                  })();
                  break;
                case "failed":
                  execution.action("workspace.answer.refused.failed", () => {
                  })();
                  break;
                case "stale":
                  execution.action("workspace.answer.refused.stale", () => {
                  })();
                  break;
                case "partial":
                  execution.action("workspace.answer.refused.partial", () => {
                  })();
                  break;
                case "unknown":
                  execution.action("workspace.answer.refused.unknown", () => {
                  })();
                  break;
              }
            },
            answerReadback: function() {
              for (const [key, r4] of answerRequests) {
                const p5 = api.project(r4.board, r4.root), t4 = p5 && p5.complete === true && p5.tasks.find((t5) => t5.n === r4.op.task);
                if (r4.reconciled || r4.op.action || !t4 || t4.state === "ask" && t4.revision === r4.op.revision) continue;
                r4.reconciled = true;
                send({ a: "answer", root: r4.root, board: r4.board, op: r4.op });
              }
            },
            // The answer card owns the slot's descendants; Preact renders the slot without children.
            answerSlot: function(t4) {
              return t4.state === "ask" ? h5("div", { "data-answer": t4.key, key: "answer:" + t4.key }) : null;
            },
            answerNotices: function(board, task) {
              const { line } = execution.module("workspace.answers.notices", { line: ([key, r4], role) => r4.answered ? h5("div", { class: "body", role, key }, "#" + r4.op.task + " \xB7 " + won(r4) + " ", h5("a", { href: r4.board + "/" + r4.op.task, "data-url": r4.board + "/" + r4.op.task }, "Board \u2197"), h5("button", { "data-a": "answer-dismiss", "data-key": key, "aria-label": "Dismiss #" + r4.op.task + " receipt", title: "Dismiss" }, "\xD7")) : h5("div", { class: "body" + (FAILED.includes(r4.state) ? " alert" : ""), role, key }, "#" + r4.op.task + (r4.about ? " " + r4.about : "") + " \xB7 " + (STATE[r4.state] || r4.state) + (r4.reason ? ": " + (r4.reason === CONFLICT ? "an earlier answer with this operation is still pending with different content" : r4.reason) : "") + (r4.next ? " \xB7 " + r4.next : "") + " ", h5("a", { href: r4.board + "/" + r4.op.task, "data-url": r4.board + "/" + r4.op.task }, "Board \u2197"), ["unknown", "failed", "partial"].includes(r4.state) && h5("button", { "data-a": "answer-retry", "data-key": key }, r4.state === "failed" && r4.reason !== CONFLICT ? "Send again" : "Check delivery"), h5("details", { class: "ids", "data-view-key": "receipt:" + key }, h5("summary", null, "Details"), "Operation " + r4.op.id + " \xB7 question revision " + r4.op.revision + (r4.reason === CONFLICT ? " \xB7 bridge: " + r4.reason : "")), !answerRequests.has(key) && h5("button", { "data-a": "answer-dismiss", "data-key": key, "aria-label": "Dismiss #" + r4.op.task + " receipt", title: "Dismiss" }, "\xD7")) });
              const shown = [...answerFeedback].filter(([, r4]) => (!board || r4.board === board) && (task == null || r4.op.task === task && !r4.answered && r4.op.action !== "steer" && (!saved[board + "#" + task]?.op || saved[board + "#" + task].op.id === r4.op.id)) && !view.S.projects.some((p5) => p5.issues === r4.board && p5.tasks.some((t4) => t4.n === r4.op.task && t4.state === "ask")));
              if (task != null) return shown.slice(-1).map((x5) => line(x5, "status"));
              const answered = shown.filter(([, r4]) => r4.state === "accepted").reverse();
              return [shown.filter(([, r4]) => r4.state !== "accepted").map((x5) => line(x5, "status")), answered.length > 0 && h5("details", { class: "group answered", "data-view-key": "answered" }, h5("summary", null, "Answered" + counter(answered.length, "muted", "Answered")), answered.map((x5) => line(x5, null)))];
            },
            // #1785 A1: a question another actor answered first names that answer, never revisions or CLI advice;
            // the status row's `answered` (§ 5 late answer) names it before this client sends anything
            answerWon: function(board, t4) {
              const r4 = t4.state === "ask" ? null : [...answerFeedback.values()].filter((r5) => r5.board === board && r5.op.task === t4.n && r5.answered).at(-1) || t4.answered && { answered: t4.answered };
              return r4 ? h5("p", { class: "body answered", role: "status" }, won(r4)) : null;
            },
            // #1918 A5: the conversation is append-only: every answered question stays as it was asked, read-only, then its
            // answer line, in board order; an open question follows as the live card
            answerHistory: function(t4, root) {
              return (t4.history || []).map(({ question: q4, answered: a4 }, i4) => {
                const row = { n: t4.n, title: t4.title, url: t4.url, state: "closed", revision: a4?.revision || 0, question: { choices: [], evidence: [], ...q4 } }, key = q4 ? a4?.revision ?? i4 : "note-" + ((t4.history_start || 0) + i4);
                const files = (a4?.files || []).length > 0 && h5("ul", { class: "conversation-files" }, a4.files.map((f4) => h5("li", { key: f4.url }, h5("a", { href: f4.url, target: "_blank", rel: "noreferrer" }, f4.name || f4.url))));
                const receipt = a4?.read === true ? h5("span", { class: "note-read", "aria-label": "Read by worker" }, " \xB7 Read") : null;
                const paged = !!historyText && (q4?.text || "").length > 32768;
                return [
                  q4 && h5("section", { key: "asked:" + key, class: "dialogue-item thread-card asked", "data-reading-key": "question:" + t4.n + ":" + key, "aria-label": "Question " + ((t4.history_start || 0) + i4 + 1) + " of #" + t4.n }, h5("p", { class: "meta document-author" }, "Worker", time.stamp(q4?.updated, "Recorded question", "Question timestamp unavailable in the history adapter")), paged && historyText(row, q4.text, "history-" + key, root), h5("div", { ref: (el) => api.readingAnchors(el, row), dangerouslySetInnerHTML: { __html: component().render("history:" + t4.n + ":" + key, row, component().fresh(row), { answer: false, manage: false, composer: true, paged, reason: "Recorded question" }) } })),
                  h5("section", { key: "answered:" + key, "data-reading-key": "answer:" + t4.n + ":" + key, class: "dialogue-item body answered", role: "status" }, h5("p", { class: "meta document-author" }, a4?.by || "Author unknown", time.stamp(a4?.at != null ? time.seconds(a4.at) : a4?.updated, q4 ? "Recorded answer" : "Owner message", "Answer timestamp unavailable in the history adapter"), receipt), h5("div", { class: "conversation-text" }, a4?.text || (files ? "" : "Answer text unavailable")), files)
                ];
              });
            },
            readingAnchors: function(el, t4) {
              if (el) for (const [i4, node] of [...el.querySelectorAll(".qac-done,.qac-problem,.qac-detail,.qac-context>summary,.qac-images,.qac-evidence,.qac-text,[data-choice]")].entries()) node.dataset.readingKey = "report:question-" + t4.n + "-" + t4.revision + "-" + i4;
            },
            // ...and while its worker runs, the owner's unsent answer (a late receipt or a kept draft) can steer it instead, in the consumer's composer `steer`
            answerSteerComposer: function(board, t4) {
              const key = board + "#" + t4.n, local = saved[key], text = steers[key] ?? local?.draft ?? "", files = local?.files || [];
              if (!steer || t4.state === "ask") return null;
              const x5 = t4.executions?.find((x6) => x6.attempt === t4.attempt), working = t4.state === "doing" && t4.role === "worker" && !!t4.attempt && x5?.state === "running", p5 = api.project(board), available = working && p5?.alive && p5.complete === true;
              if (!working && !text.trim() && !files.length) return h5("p", { class: "meta" }, t4.state === "review" ? "Reviewer input is read-only. Open the board for decisions." : "This attempt has ended or is unavailable. No worker message will be sent.");
              const changed2 = !!local?.steerAttempt && local.steerAttempt !== t4.attempt;
              const r4 = local?.op?.action === "steer" ? local.receipt : null, sending = r4?.state === "sending", unknown = r4?.state === "unknown", disabled = !available || sending || unknown;
              const reason = changed2 ? "Worker attempt changed. Draft kept; edit it to address the current worker." : !available ? "Worker input is unavailable for this attempt. Draft and files kept." : !r4 ? "Message goes to this worker through the board; delivery does not prove it was consumed." : r4.state === "accepted" ? "Message saved on the board; worker consumption is unconfirmed." : (STATE[r4.state] || r4.state) + (r4.reason ? ": " + r4.reason : "") + (r4.next ? " \xB7 " + r4.next : "");
              const problem = replyFiles.problem(files), receiptKey = p5?.root + "#" + local?.op?.id;
              return h5("section", { class: "answer-composer", "aria-label": "Steer #" + t4.n }, steer({
                text,
                disabled,
                id: "steer-draft",
                field: "steer",
                label: "Message the worker",
                placeholder: "Tell the working agent what to change",
                reason: problem || reason,
                ...api.fileEvents(board, t4.n, disabled),
                files: api.fileControls(board, t4.n, disabled),
                tools: h5("div", { class: "conversation-tools composer-bottom" }, h5("button", { "data-a": "answer-steer", "data-board": board, "data-task": t4.n, class: "primary-action", disabled: disabled || changed2 || !!problem || !text.trim() && !files.length }, "Send"), unknown && h5("button", { "data-a": "answer-retry", "data-key": receiptKey }, "Check delivery")),
                onInput: (e4) => {
                  steers[key] = e4.currentTarget.value;
                  save(board, t4, { ...saved[key] || component().fresh(t4), draft: e4.currentTarget.value, caret: [e4.currentTarget.selectionStart, e4.currentTarget.selectionEnd], steerAttempt: t4.attempt });
                  draw();
                }
              }));
            },
            answerSteer: function(board, t4) {
              const key = board + "#" + t4.n, text = (steers[key] ?? saved[key]?.draft ?? "").trim();
              if (saved[key]?.op?.action === "steer" && ["sending", "unknown"].includes(saved[key]?.receipt?.state)) return answerSend(board, saved[key].op);
              if (t4.state !== "doing" || t4.role !== "worker" || !t4.executions?.some((x5) => x5.attempt === t4.attempt && x5.state === "running") || saved[key]?.steerAttempt && saved[key].steerAttempt !== t4.attempt) return Promise.resolve(null);
              const files = saved[key]?.files || [];
              if (!text && !files.length || replyFiles.problem(files)) return Promise.resolve(null);
              steers[key] = text;
              const op = { id: "steer-" + t4.n + "-" + Date.now().toString(36), task: t4.n, revision: t4.revision, action: "steer", text, ...files.length ? { files: JSON.parse(JSON.stringify(files)) } : {} };
              save(board, t4, { ...saved[key], draft: text, op, receipt: { state: "sending" } });
              return answerSend(board, op);
            },
            answerToast: function() {
              const r4 = view.toast && answerFeedback.get(view.toast);
              return r4 ? h5("div", { class: "toast", role: "status" }, "#" + r4.op.task + " \xB7 " + (r4.note ? "task note saved" : r4.answered ? "already answered" : { start: "started", stop: "stopped", later: "parked", retry: "requeued", steer: "steer sent" }[r4.op.action] || "answer accepted") + " ", h5("a", { href: r4.board + "/" + r4.op.task, "data-url": r4.board + "/" + r4.op.task }, "Board \u2197"), h5("button", { "data-a": "toast-dismiss", "aria-label": "Dismiss", title: "Dismiss" }, "\xD7")) : null;
            },
            mountAnswers: function() {
              for (const [key, c4] of answerCards) if (!c4.el.isConnected) {
                c4.ui.update([]);
                answerCards.delete(key);
              }
              for (const slot of menu.querySelectorAll("[data-answer]")) {
                const key = slot.dataset.answer, at = key.lastIndexOf("/"), p5 = api.project(key.slice(0, at)), row = p5?.tasks.find((t5) => t5.n === Number(key.slice(at + 1)));
                if (!row) continue;
                const t4 = read(p5, { ...row, key, url: row.url || p5.issues + "/" + row.n });
                if (t4.body_lazy) continue;
                let card = answerCards.get(t4.key);
                if (!card) {
                  const el = document2.createElement("div");
                  card = { el, ui: component().mount(el, { capability: answerCapability, send: answerSend, load, save }) };
                  answerCards.set(t4.key, card);
                }
                if (card.el.parentNode !== slot) slot.append(card.el);
                card.ui.update([{ board: p5.issues, row: t4 }]);
                api.readingAnchors(card.el, t4);
                for (const d5 of slot.querySelectorAll("details")) {
                  const key2 = "answer:" + t4.key + ":" + d5.className;
                  if (d5.dataset.viewKey !== key2) d5.dataset.viewKey = key2;
                }
              }
            },
            answerSend: function(board, op) {
              const kept = saved[board + "#" + op.task];
              const p5 = api.project(board, kept?.op?.id === op.id ? kept.projectRoot : void 0);
              if (!p5) {
                api.answerRefusal("rejected");
                return Promise.resolve({ state: "rejected", reason: "Project is no longer registered", next: "Select a registered project, then send again" });
              }
              const key = p5.root + "#" + op.id;
              if (answerRequests.has(key)) return answerRequests.get(key).promise;
              let resolve;
              const promise = new Promise((r4) => resolve = r4);
              const timer = defer(() => {
                answerReceipt({ root: p5.root, id: op.id, state: "unknown", reason: "No bridge receipt received", next: "Check delivery with the same operation" });
              }, 3e4);
              answerRequests.set(key, { promise, resolve, timer, board, op, root: p5.root });
              answerFeedback.set(key, { board, op, about: kept?.op?.id === op.id ? kept.about : void 0, state: "sending", reason: "" });
              send({ a: "answer", root: p5.root, board, op, local: saved[board + "#" + op.task] });
              return promise;
            },
            recovery: function() {
              return saved;
            },
            restoreRecovery: function(locals) {
              for (const key of Object.keys(locals || {})) {
                delete saved[key];
                delete steers[key];
                const at = key.lastIndexOf("#"), id = key.slice(0, at) + "/" + key.slice(at + 1), card = answerCards.get(id);
                if (card) {
                  card.ui.update([]);
                  card.el.remove();
                  answerCards.delete(id);
                }
              }
              api.hydrate(locals);
            },
            steerCaret: function(board, n3, caret) {
              const local = saved[board + "#" + n3];
              if (local && JSON.stringify(local.caret) !== JSON.stringify(caret)) save(board, { n: n3 }, { ...local, caret });
            },
            bounds: function() {
              return { controllers: answerCards.size, protectedSources: Object.keys(saved).length, protectedBytes: new TextEncoder().encode(JSON.stringify(saved)).length };
            },
            load: function(board, row) {
              return saved[board + "#" + row.n];
            },
            save: function(board, row, local) {
              const prior = saved[board + "#" + row.n], projectRoot = prior?.op?.id && prior.op.id === local.op?.id ? prior.projectRoot : api.project(board)?.root;
              saved[board + "#" + row.n] = JSON.parse(JSON.stringify({ ...local, projectRoot }));
              send({ a: "answer-local", key: board + "#" + row.n, local: saved[board + "#" + row.n] });
              changed?.();
            },
            hydrate: function(locals) {
              for (const [key, local] of Object.entries(locals || {})) if (!Object.hasOwn(saved, key) && local && typeof local.draft === "string") {
                saved[key] = { ...local, confirmClose: false };
                if (local.op && ["sending", "unknown"].includes(local.receipt?.state)) saved[key].receipt = { state: "unknown", reason: "Workspace reopened; delivery needs readback", next: "Check delivery with the same operation" };
                const board = key.slice(0, key.lastIndexOf("#")), root = api.project(board, local.projectRoot)?.root || local.projectRoot;
                if (local.op && root && saved[key].receipt) answerFeedback.set(root + "#" + local.op.id, { board, op: local.op, about: local.about, ...saved[key].receipt });
              }
            },
            retry: function(key) {
              const r4 = answerFeedback.get(key);
              if (r4) ui.effect("workspace.answer.reconcile", () => answerSend(r4.board, r4.op), () => {
              });
            },
            startPending: function(board, t4) {
              return [...answerFeedback.values()].some((r4) => r4.board === board && r4.op.task === t4.n && r4.op.revision === t4.revision && r4.op.action === "start" && !["failed", "stale", "rejected"].includes(r4.state));
            },
            dismiss: function(key) {
              answerFeedback.delete(key);
              if (view.toast === key) view.toast = null;
              draw();
            }
          });
          const { answerCapability, answerReceipt, answerReadback, answerSlot, answerNotices, answerToast, mountAnswers, answerSend, load, save } = api;
          const CONFLICT = "Correlation id is already pending for a different request";
          const won = (r4) => "Answered by " + (r4.answered.by || "another client") + ": " + r4.answered.text + (r4.state === "accepted" && r4.reason ? " \xB7 " + r4.reason : "");
          const FAILED = ["rejected", "unknown", "failed", "partial"];
          const STATE = { sending: "sending", accepted: "accepted", rejected: "not accepted", stale: "task changed; action not sent", unknown: "delivery unknown", failed: "not sent", partial: "partly applied" };
          const component = () => require_answer_card();
          const { tasks, unique, counter } = execution.module("workspace.answers.helpers", {
            tasks: (p5) => (p5.tasks || []).map((t4) => ({ ...t4, key: p5.issues + "/" + t4.n, url: t4.url || p5.issues + "/" + t4.n })),
            unique: (ts) => [...new Map(ts.map((t4) => [t4.key, t4])).values()],
            counter: (n3) => " \xB7 " + n3
          });
          return api;
        }
      });
      module.exports = { create };
    }
  });

  // src/tray-app.js
  var require_tray_app = __commonJS({
    "src/tray-app.js"(exports, module) {
      "use strict";
      var ui = require_ui_runtime();
      var execution = ui.execution;
      var shell = require_app_shell();
      var { h: h5, Fragment } = (init_preact(), __toCommonJS(preact_exports));
      var { icon, button } = require_ui_icons2();
      var syncView = require_sync_view();
      var { createState, create } = execution.module("tray", {
        createState: function(snapshot = null) {
          return { open: null, dashboard: null, popup: null, manualSize: null, agentqRuntime: null, agentqError: null, aiRoot: null, addConfirm: false, resourceNotice: null, persistenceError: null, toast: null, receivedAt: 0, S: snapshot, page: "home", taskFilter: null, working: null, workCard: null, ask: null, noted: null, pending: {}, hostCommand: null, recovery: {}, confirming: null, updating: null, draft: null, disclosures: {}, pressedAt: null, stale: false, resizing: null };
        },
        create: function(view, ports) {
          const api = {
            ...execution.module("tray.create", { mode: function(p5) {
              return p5.mode;
            } }),
            operationLauncher: function(operation, root, label) {
              return h5("button", { class: "project", "data-a": "workspace", "data-page": "operations", "data-origin": ports.settings ? view.page : void 0, "data-operation": operation, "data-root": root || null }, label);
            },
            operationTarget: function(d5) {
              return { work: "resources", confirm: "admission", "pause-all": "admission", "resume-all": "admission", pause: "admission", resume: "admission", focus: "admission", pick: "admission", add: "setup", "add-ask": "setup", "add-install": "setup", "add-setup": "setup", "add-cancel": "setup", "stop-review": "stop", stop: "stop", "stop-full": "stop", restart: "service", uninstall: "service", update: "update", "recovery-read": "recovery", "recovery-apply": "recovery", "recovery-reconcile": "recovery", "recovery-pending": "recovery" }[d5.a];
            },
            operationBody: function(context) {
              const c4 = context || {}, ps = view.S.projects, p5 = ps.find((p6) => p6.root === c4.root), host = view.S.host || "unknown";
              if (c4.root && !p5) return f4(actionNotices(), h5("p", { role: "status" }, "Project unavailable; no operation can be applied. Return to Messages or Refresh."));
              const selected = p5 ? [p5] : ps;
              const heading = f4(h5("p", { class: "meta" }, "Host " + host + " \xB7 " + (p5 ? p5.name : selected.length + " registered projects")), h5("details", { "data-view-key": "operation-targets" }, h5("summary", null, "Targets"), selected.map((p6) => h5("div", { class: "body meta" }, p6.name + " \xB7 " + p6.root))));
              let body;
              switch (c4.operation) {
                case "service":
                  body = [h5("button", { class: "project", "data-a": "resources" }, "Open resource monitor"), view.resourceNotice && h5("p", { role: "status" }, view.resourceNotice.reason), api.hostService(), p5 ? [service(p5, false), capacity(p5), workEntry(p5.root)] : ps.map((p6) => service(p6, false))];
                  break;
                case "resources": {
                  const target = p5 || ps.find((p6) => p6.resources);
                  body = target ? [h5("p", null, "Host limits affect every registered project; project ceilings remain read-only."), api.workPage(target.root)] : h5("p", { role: "status" }, "Capacity unavailable: no supported host projection.");
                  break;
                }
                case "stop":
                  body = view.S.stop_modes ? api.stopPanel() : h5("p", { role: "status" }, "Emergency stop unavailable on this host. Normal stop is under Services.");
                  break;
                case "recovery":
                  body = view.S.stop_modes ? api.recoveryPanel() : h5("p", { role: "status" }, "Reviewed recovery unavailable on this host.");
                  break;
                case "admission": {
                  const selected2 = p5 ? [p5] : ps;
                  body = [h5("p", { class: "body", role: "status" }, api.admissionText(selected2)), h5("p", null, "Pause holds new admission; current tasks finish. Resume restores saved focus and cannot clear an emergency hold."), selected2.map((p6) => h5("section", { key: p6.root }, h5("p", null, p6.name + " \xB7 " + modeText(p6)), admission(p6))), !p5 && bulk(selected2.filter((p6) => mode(p6) === "run"), selected2.filter((p6) => mode(p6) === "paused"))];
                  break;
                }
                case "update":
                  body = [h5("p", null, "Request the advertised update on this host. Running work is preserved; success requires the version read back from service status."), update()];
                  break;
                case "agentq":
                  body = [api.agentq(true)];
                  break;
                case "setup":
                  body = [h5("p", null, "Choose a folder using the native picker. Opening or leaving this review registers nothing. Setup must confirm account, repository, visibility and exact external effects under \xA7 6 before applying them."), addProject(), setup()];
                  break;
                default:
                  body = h5("p", { role: "status" }, "Operation unavailable; no effect was submitted.");
              }
              return f4(heading, actionNotices(), body);
            },
            operationAction: function(d5) {
              if (d5.a === "work" || d5.a === "workspace" && d5.page === "operations") {
                const next = { operation: d5.a === "work" ? "resources" : d5.operation, root: d5.root || null };
                if (ports.navigate) ports.navigate("operations", next);
                else if (ports.workspace) {
                  view.open = { page: "operations", ...next };
                  present();
                } else return false;
                return true;
              }
              if (d5.a === "agentq-runtime") {
                view.agentqRuntime = d5.runtime;
                present();
                return true;
              }
              if (d5.a === "agentq-dismiss" || d5.a === "agentq-check") {
                send({ a: d5.a });
                return true;
              }
              if (d5.a === "agentq-setup") {
                const A4 = view.S.agentq, rows = A4?.runtimes || [], runtime = view.agentqRuntime || rows.find((r4) => r4.eligible)?.runtime;
                if (!rows.some((r4) => r4.runtime === runtime && r4.eligible)) return true;
                const id = Array.from(crypto2.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join("");
                send({ a: d5.a, id, runtime });
                present();
                return true;
              }
              if (d5.a === "add-ask" || d5.a === "add-back") {
                view.addConfirm = d5.a === "add-ask";
                present();
                return true;
              }
              if (d5.a === "add-install" || d5.a === "add-cancel") view.addConfirm = false;
              if (d5.a === "confirm" || d5.a === "cancel") {
                view.confirming = d5.a === "confirm";
                present();
                menu.querySelector(view.confirming ? '[data-a="cancel"]' : '[data-a="confirm"]')?.focus();
                return true;
              }
              if (d5.a === "update") {
                if (view.updating) return true;
                view.updating = { v: d5.v, root: d5.root, at: view.S.now };
                send({ a: "update", root: d5.root });
                present();
                return true;
              }
              if (d5.a === "pause-all" || d5.a === "resume-all") {
                view.confirming = null;
                for (const p5 of view.S.projects.filter((p6) => mode(p6) === (d5.a === "pause-all" ? "run" : "paused"))) focusStart(d5.a.slice(0, -4), p5.root);
                present();
                menu.querySelector('[data-a="confirm"]')?.focus();
                return true;
              }
              if (d5.a === "recovery-pending") {
                const item = view.recovery[d5.root], request = item?.review?.pending?.[Number(d5.index)];
                if (request) {
                  item.request = request;
                  api.recoveryStart(d5.root, "recovery-reconcile");
                }
                return true;
              }
              if (["recovery-read", "recovery-apply", "recovery-reconcile"].includes(d5.a)) {
                api.recoveryStart(d5.root, d5.a);
                return true;
              }
              if (d5.a === "stop-full") {
                api.lifecycleStart(d5.root, "stop", "full");
                return true;
              }
              if (d5.a === "restart" || d5.a === "stop" || d5.a === "uninstall") {
                api.lifecycleStart(d5.root, d5.a);
                return true;
              }
              if (d5.a === "pause" || d5.a === "resume" || d5.a === "focus") {
                focusStart(d5.a, d5.root, d5.label);
                view.ask = null;
                present();
                return true;
              }
              if (d5.a === "pick") {
                view.ask = d5.root;
                present();
                return true;
              }
              if (["add", "add-install", "add-cancel", "add-setup", "resources", "log", "folder"].includes(d5.a)) {
                if (["add-install", "add-setup"].includes(d5.a)) {
                  const A4 = view.S.add;
                  if (!A4 || view.setupPending === A4.generation) return true;
                  view.setupPending = A4.generation;
                  send({ a: d5.a, generation: A4.generation });
                } else send({ a: d5.a, root: d5.root });
                present();
                return true;
              }
              return false;
            },
            operationInput: function(e4) {
              const d5 = e4.target.dataset;
              if (d5.recoveryTask || d5.recoveryEvidence) {
                const item = view.recovery[d5.root];
                if (!item) return;
                const key = d5.recoveryTask || d5.recoveryEvidence, draft = item.drafts[key] || (item.drafts[key] = {});
                if (d5.recoveryTask) draft.selected = e4.target.checked;
                else draft.evidence = e4.target.value;
                item.notice = "";
              }
            },
            gutter: function() {
              if (ports.measureScrollbar) return ports.measureScrollbar();
              const d5 = document2.createElement("div");
              d5.style.cssText = "position:absolute;top:-99px;width:60px;height:60px;overflow:scroll;scrollbar-width:thin";
              document2.body.append(d5);
              const g5 = d5.offsetWidth - d5.clientWidth;
              d5.remove();
              return Math.ceil(g5);
            },
            // #1523: the window's own layout (header, chips, summary, list, footer, any new panel) with the scroll area at full content height.
            natural: function() {
              const c4 = menu.querySelector(".content"), b5 = c4 && c4.querySelector(".content-body"), all = menu.getBoundingClientRect().height;
              if (!b5) return Math.ceil(all);
              const cs = getComputedStyle(c4), edges = ["paddingTop", "paddingBottom", "borderTopWidth", "borderBottomWidth"].reduce((n3, k5) => n3 + (parseFloat(cs[k5]) || 0), 0);
              return Math.ceil(all - c4.getBoundingClientRect().height + b5.getBoundingClientRect().height + edges);
            },
            fit: function() {
              if (ports.workspace || ports.settings || ports.views) return;
              const size = view.manualSize || view.S.size, fixed = (view.S.size_anchor || (size ? "fixed" : "top")) === "fixed", w5 = view.page === "recovery" ? Math.min(720, window2.screen?.availWidth || 720) : size ? size[0] : view.S.font >= 18 ? 360 : 320;
              document2.body.style.width = "100%";
              menu.style.width = w5 + "px";
              send({ a: "size", d: window2.devicePixelRatio, g: fixed && size ? 0 : gutter(), w: w5, h: fixed && size ? size[1] : natural() });
              menu.style.width = "100%";
            },
            actionReceipt: function(r4) {
              if (!view.S?.projects.some((p5) => p5.root === r4.root)) return;
              actionErrors.set(r4.root, { text: r4.text });
              present();
            },
            actionNotices: function() {
              if (!ports.workspace) return view.workspaceError ? h5("p", { role: "alert" }, view.workspaceError) : "";
              const r4 = view.hostCommand || view.S?.lifecycle;
              const alert = (text) => h5("div", { class: "body alert", role: "alert" }, text);
              const items = [view.aiError && alert(view.aiError), view.workspaceError && alert(view.workspaceError), ...view.S.projects.filter((p5) => actionErrors.has(p5.root)).map((p5) => alert(p5.name + ": " + actionErrors.get(p5.root).text)), r4?.action === "uninstall" && h5("div", { class: "body meta", role: "status" }, "Uninstall " + (r4.id || "") + ": " + (r4.state || "unknown") + (r4.reason ? " \xB7 " + r4.reason : ""))].filter(Boolean);
              return items.length ? f4(items) : "";
            },
            lifecycleStart: function(root, action = "restart", mode2 = "drain") {
              if (view.hostCommand && view.hostCommand.state !== "observed") return;
              const review = action === "uninstall" ? view.S.projects.find((p5) => p5.root === root)?.uninstall_review : action === "stop" ? mode2 === "full" ? view.S.full_stop_review : view.S.stop_review : view.S.restart_review;
              if (!review || typeof review.instance !== "string" || !Array.isArray(review.roots)) {
                actionErrors.set(root, { text: review?.reason || "Lifecycle review unavailable; check host service status." });
                present();
                return;
              }
              const id = Array.from(crypto2.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join("");
              const item = { id, root, project: review.project, action, mode: mode2, stops: action === "uninstall" && review.roots.length === 1, instance: review.instance, state: "sending", unknown: false };
              view.hostCommand = item;
              actionErrors.delete(root);
              defer(() => {
                if (view.hostCommand !== item || item.state === "observed") return;
                item.unknown = true;
                item.state = "unknown";
                item.reason = ({ stop: "Stop", uninstall: "Uninstall" }[action] || "Restart") + " confirmation unknown; check service status for " + id + ".";
                present();
              }, 3e4);
              send({ ...review, id, a: action, root });
              present();
            },
            lifecycleReceipt: function(r4) {
              const item = view.hostCommand;
              if (!item || r4.id !== item.id) return;
              if (!["queued", "applied", "observed"].includes(r4.state)) {
                item.unknown = true;
                item.state = "unknown";
                item.reason = r4.reason || "Lifecycle outcome unknown; check service status.";
              } else if (r4.state === "observed" && !api.lifecycleObserved(r4, item.id, item.instance, item.action, item.project, item.stops)) {
                item.unknown = true;
                item.state = "unknown";
                item.reason = "Lifecycle receipt is incomplete; check service status.";
              } else {
                item.state = r4.state;
                item.reason = r4.reason || "";
              }
              api.lifecycleReadback(r4);
              present();
            },
            lifecycleObserved: function(r4, id, origin, action = "restart", project2 = null, stops = false) {
              const receipts = (!r4?.action || r4.action === action) && r4?.id === id && r4.state === "observed" && ["received_receipt", "applied_receipt", "observed_receipt"].every((k5) => typeof r4[k5] === "string" && /^[a-f0-9]{32}$/.test(r4[k5]));
              if (!receipts) return false;
              if (action === "uninstall" && (r4.action !== "uninstall" || r4.origin_instance !== origin || r4.project !== project2 || r4.unregistered !== true)) return false;
              if (action === "uninstall" && !stops) return typeof r4.service_instance === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(r4.service_instance);
              if (action === "stop" || action === "uninstall") return r4.action === action && r4.origin_instance === origin && (action !== "uninstall" || r4.unit_removed === true) && r4.native_exit === true && Number.isInteger(r4.native_pid) && r4.native_pid > 0 && typeof r4.native_started === "string" && /^[A-Za-z0-9-]{1,128}$/.test(r4.native_started) && typeof r4.unit_revision === "string" && /^[a-f0-9]{64}$/.test(r4.unit_revision);
              return typeof r4.service_instance === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(r4.service_instance) && typeof origin === "string" && r4.service_instance !== origin;
            },
            lifecycleReadback: function(r4 = view.S?.lifecycle) {
              const item = view.hostCommand;
              if (item && r4?.id === item.id && ["queued", "applied"].includes(r4.state)) {
                item.state = r4.state;
                item.reason = r4.reason || "";
              }
              if (item && api.lifecycleObserved(r4, item.id, item.instance, item.action, item.project, item.stops)) {
                item.state = "observed";
                item.unknown = false;
                item.reason = item.action === "uninstall" ? "Project removal observed." + (item.stops ? " Host unit removal and exact exit observed." : " Other projects continue.") : item.action === "stop" ? "Recorded host exit observed; restart: " + (r4.restart_condition || "check native autostart conditions") + "." : "New service generation observed.";
              }
            },
            lifecycleWaits: function() {
              if (!Array.isArray(view.S?.projects)) return h5("div", { class: "body meta" }, "Project registry unknown");
              return f4(view.S.projects.map((p5) => {
                const current = view.S.service?.state === "running" && typeof p5.service_pid === "number" && typeof p5.host_instance === "string" && p5.alive === true && p5.service_pid === view.S.service?.pid && p5.host_instance === view.S.service?.instance;
                const text = !current ? "waiting for current project observation" : p5.error ? p5.error : Array.isArray(p5.restart_waits) ? p5.restart_waits.length ? "waits for " + p5.restart_waits.join(", ") : "prepared" : "waiting for recovery observation";
                return h5("div", { class: "body meta" }, p5.name + ": " + text);
              }));
            },
            lifecycleControl: function(p5, action = "restart") {
              if (!ports.workspace) return api.operationLauncher("service", p5.root, "Review " + p5.name + " service\u2026");
              const title = { stop: "Stop", uninstall: "Uninstall" }[action] || "Restart", review = action === "uninstall" ? p5.uninstall_review : action === "stop" ? view.S?.stop_review : view.S?.restart_review;
              const all = view.hostCommand || view.S?.lifecycle, r4 = (all?.action || "restart") === action && (action !== "uninstall" || (all.root || all.project) === p5.root) ? all : null;
              const pending = view.hostCommand && view.hostCommand.state !== "observed" || ["queued", "applied"].includes(view.S?.lifecycle?.state);
              const projects2 = Array.isArray(view.S?.projects) ? view.S.projects : [];
              const targets = Array.isArray(review?.roots) && review.roots.every((root) => typeof root === "string") ? review.roots.map((root) => (projects2.find((p6) => p6.root === root)?.name || "Project") + " (" + root + ")") : null;
              const effect = action === "uninstall" ? "Waits for this project\u2019s owned work to end; files and other projects stay." + (targets?.length === 1 ? " Removes the last host unit after exact exit." : "") : action === "stop" ? view.S?.stop_modes ? "Drain: durable hold; current work finishes. Automatic rollback to older releases is disabled. Use reviewed recovery to resume selected tasks." : "Waits for all owned work to end; retains OS autostart. Explicit service install can start it again." : "Existing work is preserved; active turns can delay restart.";
              const scope = action === "uninstall" ? p5.name : "all registered projects: " + (targets ? projects2.map((p6) => p6.name).join(", ") || "none registered" : "unknown");
              return h5("details", { class: "host-" + action, "data-view-key": p5.root + ":" + action }, h5("summary", null, icon({ restart: "rotate-cw", stop: "square", uninstall: "trash-2" }[action]), title + " " + (action === "uninstall" ? p5.name : "host service")), h5("div", { class: "body meta" }, "Affects " + scope + ". " + effect), h5("details", { "data-view-key": p5.root + ":" + action + ":targets" }, h5("summary", null, "Exact targets"), targets && targets.map((t4) => h5("div", { class: "body meta" }, t4))), !review?.instance && h5("div", { class: "body alert" }, review?.reason || title + " protocol unavailable"), h5("button", { class: "project", "data-a": action, "data-root": p5.root, disabled: !!(pending || !review?.instance || !targets) }, "Confirm " + (action === "uninstall" ? "uninstall " + p5.name : "host " + action)), pending && r4 && api.lifecycleWaits(), (r4?.id || r4?.reason) && h5("div", { class: "body meta", role: "status" }, title + " " + (r4.id || "") + ": " + (r4.state || "unknown") + (r4.reason ? " \xB7 " + r4.reason : "")));
            },
            stopPanel: function() {
              if (!ports.workspace) return api.operationLauncher("stop", null, "Review Stop and hold\u2026");
              const projects2 = view.S.projects || [], root = projects2[0]?.root, held = view.S.stop_hold;
              const pending = !!held && !held.selected_count || view.hostCommand && view.hostCommand.state !== "observed" || ["queued", "applied"].includes(view.S.lifecycle?.state);
              const names = projects2.length + " registered projects";
              const host = projects2[0]?.host || "unknown host", command = view.hostCommand || view.S.lifecycle;
              const rows = projects2.map((p5) => h5("div", { class: "body" }, h5("strong", null, p5.name), h5("div", { class: "meta" }, view.S.service?.state !== "running" || p5.alive !== true || p5.service_pid !== view.S.service?.pid || p5.host_instance !== view.S.service?.instance ? "Process state unknown: service observation stale or from another generation." : Array.isArray(p5.stop_progress) && p5.stop_progress.length ? lines(p5.stop_progress.map((k5) => "#" + k5.task + " " + k5.rt + ": " + k5.state + (k5.reason ? " \xB7 " + k5.reason : ""))) : "No stop acknowledgement; running or unknown work may remain.")));
              return f4(
                h5("div", { class: "body" }, "Stop on " + host + " \xB7 " + names),
                h5("div", { class: "body meta" }, "Only TaskQ-owned local work is targeted. Remote hosts and unrelated agents are unaffected. Dirty files and session references are retained; unsaved buffers and interrupted external effects may be lost. Automatic rollback to older releases is disabled."),
                h5("div", { class: "body" }, h5("strong", null, "Drain"), h5("div", { class: "meta" }, "Hold new admission and let current work finish. Retain autostart and the hold after service exit.")),
                h5("button", { class: "project", "data-a": "stop", "data-root": root || "", disabled: !!(pending || !root || !view.S.stop_review?.instance) }, "Confirm Drain"),
                h5("div", { class: "body" }, h5("strong", null, "Full stop"), h5("div", { class: "meta" }, "Request cancellation now, including offline. Unknown termination stays fenced. Board reconciliation may keep the service alive.")),
                h5("button", { class: "project", "data-a": "stop-full", "data-root": root || "", disabled: !!(pending || !root || !view.S.full_stop_review?.instance) }, "Confirm Full stop"),
                !view.S.full_stop_review?.instance && h5("div", { class: "body alert" }, view.S.full_stop_review?.reason || "Full stop capability unknown"),
                held && h5("div", { class: "body alert", role: "status" }, "Admission held: " + (held.mode || "unknown") + "; ordinary Play cannot clear this hold."),
                held && h5("button", { class: "project", "data-route": "recovery" }, "Review recovery"),
                command?.action === "stop" && h5("div", { class: "body meta", role: "status" }, "Stop " + (command.id || "") + ": " + (command.state || "unknown") + (command.reason ? " \xB7 " + command.reason : "")),
                h5("details", { "data-view-key": "stop-projects" }, api.disclosure("folder", "Project observations"), rows),
                api.lifecycleWaits()
              );
            },
            recoveryPanel: function() {
              if (!ports.workspace) return api.operationLauncher("recovery", null, "Review local task recovery\u2026");
              return f4(h5("div", { class: "body" }, "Selected local tasks only \xB7 Keep held by default. Unknown effects must remain held."), h5("details", { "data-view-key": "recovery-scope" }, h5("summary", null, "Recovery scope and effects"), h5("div", { class: "body meta" }, "Paused projects, other tasks and operators stay held. Inspect native release, work and external effects before confirming. Restart discards the old result and starts from preserved files.")), view.S.projects.map((p5) => {
                const item = view.recovery[p5.root], review = item?.review;
                const read = item?.pending ? "Reading\u2026" : review && item.readAt ? "Read at " + when(item.readAt) : item?.notice ? "Outcome unknown" : "Not read";
                const next = item?.pending ? "wait for the receipt" : !review ? "read current recovery" : review.rows.some((r4) => r4.eligible && !r4.selected) ? "select tasks, record effects, then confirm" : "nothing eligible; work stays held";
                return h5(
                  "section",
                  { class: "group", "data-recovery-project": p5.root, key: p5.root },
                  h5("div", { class: "body" }, h5("strong", null, p5.name), h5("div", { class: "meta", role: "status" }, read + " \xB7 Next: " + next)),
                  h5("button", { "data-a": "recovery-read", "data-root": p5.root, disabled: !!item?.pending }, "Read current recovery"),
                  item?.notice && h5("div", { class: "body alert", role: "status" }, item.notice),
                  review && [
                    h5("div", { class: "body meta" }, review.mode + " \xB7 " + review.rows.length + " tasks \xB7 " + review.rows.filter((r4) => r4.eligible && !r4.selected).length + " eligible"),
                    h5(
                      "details",
                      { "data-view-key": "recovery-policy:" + p5.root },
                      h5("summary", null, "Focus, limits and access"),
                      h5("div", { class: "body meta" }, "Focus " + ((review.focus || []).join(", ") || "off")),
                      h5("div", { class: "body meta" }, "Limits: " + (review.limits?.unknown || Object.entries(review.limits?.limits || {}).map(([k5, v4]) => k5 + " " + v4).join(", ") || "unknown"), h5("br"), "Access: " + ((review.access || []).map((a4) => a4.runtime + ": " + (a4.refused || a4.effective || "unknown")).join("; ") || "unknown"))
                    ),
                    review.rows.map((r4) => {
                      const draft = item.drafts?.[r4.task] || {}, blocked = !r4.eligible || r4.selected || item.pending || (review.pending || []).some((q4) => q4.rows.some((s4) => s4.task === r4.task));
                      return h5("div", { class: "body", key: r4.task }, h5("label", null, h5("input", { type: "checkbox", "data-recovery-task": r4.task, "data-root": p5.root, checked: !!draft.selected, disabled: !!blocked }), " #" + r4.task + " " + (r4.title || "Task")), h5("div", { class: "meta" }, (r4.runtime || "unknown runtime") + " \xB7 " + (r4.host || "unknown host") + " \xB7 " + (r4.native_exit || "unknown"), h5("br"), r4.workspace?.state || "work unknown", r4.blocker && [h5("br"), r4.blocker], r4.selected && [h5("br"), "Selected admission confirmed; running state is separate."]), h5("details", { "data-view-key": "recovery-execution:" + p5.root + ":" + r4.task }, h5("summary", null, "Execution details"), h5("div", { class: "body meta" }, "Execution " + (r4.execution ?? "unknown") + " \xB7 revision " + (r4.revision ?? "unknown")), h5("a", { href: r4.url || "#", "data-url": r4.url || "" }, "Inspect task and receipts \u2197")), h5("label", { class: "meta recovery-evidence" }, "Reconciled effect outcomes", h5("input", { type: "text", maxlength: "160", "data-recovery-evidence": r4.task, "data-root": p5.root, "aria-label": "Effect evidence for task " + r4.task, value: draft.evidence || "", disabled: !!blocked })));
                    }),
                    (review.pending || []).map((q4, i4) => h5("button", { "data-a": "recovery-pending", "data-root": p5.root, "data-index": i4, disabled: !!item.pending }, "Reconcile saved selection #" + q4.rows[0].task)),
                    h5("div", { class: "body meta" }, "Confirm only when the evidence establishes the effects; native release remains mandatory. Current grants, limits and focus still apply. If the original host exit is verified, confirmation starts host supervision once."),
                    h5("button", { "data-a": "recovery-apply", "data-root": p5.root, disabled: !!item.pending }, "Confirm selected recovery")
                  ],
                  item?.request && !item.pending && h5("button", { "data-a": "recovery-reconcile", "data-root": p5.root }, "Reconcile same selection")
                );
              }));
            },
            recoveryStart: function(root, action) {
              const item = view.recovery[root] || (view.recovery[root] = { drafts: {} });
              if (item.pending) return;
              let request;
              if (action === "recovery-reconcile") {
                request = item.request;
                if (!request) return;
                action = "recovery-apply";
              } else if (action === "recovery-apply") {
                const review = item.review;
                if (!review) return;
                const rows = review.rows.filter((r4) => r4.eligible && !r4.selected && item.drafts[r4.task]?.selected).map((r4) => ({ task: r4.task, execution: r4.execution, revision: r4.revision, workspace: r4.workspace.fingerprint, evidence: item.drafts[r4.task].evidence || "", effects_known: true }));
                if (!rows.length || rows.length > 8 || rows.some((r4) => !r4.evidence.trim())) {
                  item.notice = "Select 1\u20138 eligible tasks and record verified effect outcomes for each.";
                  present();
                  return;
                }
                request = { stop: review.stop, board: review.board, rows };
                item.request = JSON.parse(JSON.stringify(request));
              }
              item.id = Array.from(crypto2.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join("");
              item.pending = true;
              item.notice = action === "recovery-read" ? "Reading current evidence\u2026" : "Applying selected recovery\u2026";
              const id = item.id;
              send({ a: action, root, id, ...request ? { request } : {} });
              defer(() => {
                if (item.id !== id || !item.pending) return;
                item.pending = false;
                item.notice = "Outcome unknown. Reconcile the same selection; do not submit changed evidence.";
                present();
              }, 65e3);
              present();
            },
            recoveryReceipt: function(r4) {
              const item = view.recovery[r4.root];
              if (!item || r4.id !== item.id) return false;
              item.pending = false;
              if (r4.state === "review" && Array.isArray(r4.rows)) {
                item.review = r4;
                item.drafts = {};
                item.readAt = view.S.now;
                item.notice = "Review current evidence; nothing is selected.";
              } else if (r4.state === "mixed" && Array.isArray(r4.receipts)) {
                item.notice = r4.receipts.map((x5) => "#" + x5.task + ": " + x5.outcome + (x5.reason ? " \xB7 " + x5.reason : "")).join("; ") + " \xB7 Mixed; unselected work stays held." + (r4.service ? " Service: " + r4.service.state + (r4.service.reason ? " \xB7 " + r4.service.reason : "") : "");
                for (const receipt of r4.receipts) if (receipt.outcome === "accepted") {
                  const row = item.review?.rows.find((r5) => r5.task === receipt.task);
                  if (row) row.selected = true;
                }
              } else item.notice = r4.reason || "Recovery outcome unknown; reconcile the same selection.";
              present();
              return true;
            },
            focusStart: function(action, root, label) {
              if (view.pending[root] && !view.pending[root].unknown) return;
              const item = { id: Array.from(crypto2.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join(""), text: action === "pause" ? "Pausing" : "Resuming", receipt: null, unknown: false };
              view.pending[root] = item;
              actionErrors.delete(root);
              defer(() => {
                if (view.pending[root] !== item) return;
                item.unknown = true;
                actionErrors.set(root, { id: item.id, text: "Command confirmation unknown. Check service status before another command." });
                present();
              }, 3e4);
              send({ a: action, root, label, id: item.id });
            },
            focusReceipt: function(r4) {
              const item = view.pending[r4.root];
              if (!view.S?.projects.some((p5) => p5.root === r4.root) || !item || item.id !== r4.id) return;
              if (typeof r4.applied_receipt === "string" && /^[a-f0-9]{32}$/.test(r4.applied_receipt)) {
                item.receipt = r4.applied_receipt;
                item.text = "Waiting for service";
                focusReadback();
              } else {
                delete view.pending[r4.root];
                actionErrors.set(r4.root, { id: item.id, text: r4.text || "Command confirmation unavailable. Check service status." });
              }
              present();
            },
            focusReadback: function() {
              for (const p5 of view.S.projects) {
                const item = view.pending[p5.root], ack = p5.focus_command;
                if (item?.receipt && p5.alive === true && ack?.state === "observed" && ack.applied_receipt === item.receipt && typeof ack.observed_receipt === "string" && /^[a-f0-9]{32}$/.test(ack.observed_receipt)) {
                  delete view.pending[p5.root];
                  if (actionErrors.get(p5.root)?.id === item.id) actionErrors.delete(p5.root);
                }
              }
              for (const root of Object.keys(view.pending)) if (!view.S.projects.some((p5) => p5.root === root)) delete view.pending[root];
            },
            remember: function() {
              return { ...viewState.capture(), workCard: view.workCard };
            },
            navigate: function(next, filter = null, work = null) {
              view.popup = null;
              if (next === "notifications") {
                send({ a: "workspace", open: { page: "conversations", filter: "new" } });
                return;
              }
              if (view.settingsWindow && !ports.settings && settingsPages.includes(next)) {
                viewState.flush();
                send({ a: "settings-open", page: next === "settings" ? null : next });
                return;
              }
              if (ports.settings && next === "home") next = "settings";
              if (!ports.workspace && (work || next === "stop" || next === "recovery" && !ports.settings)) {
                send({ a: "workspace", open: { page: "operations", operation: work ? "resources" : next, root: work ? Object.keys(work)[0] || null : null } });
                return;
              }
              renderer.push(remember());
              view.page = next;
              view.taskFilter = filter === null ? null : Array.isArray(filter) ? filter : [filter];
              view.working = work;
              view.workCard = null;
              view.ask = null;
              view.noted = null;
              view.disclosures = {};
              view.restoreScroll = 0;
              view.unavailable = null;
              present(true);
              (menu.querySelector(".content") || document2.scrollingElement).scrollTop = 0;
              menu.querySelector("h1")?.focus();
            },
            back: function() {
              const prior = renderer.pop() || { page: ports.settings || settingsPages.includes(view.page) && view.page !== "settings" ? "settings" : view.S?.projects.some((p5) => p5.root === view.page) ? view.taskFilter || view.working ? view.page : "projects" : "home", taskFilter: null, working: null, workCard: null, scroll: 0, open: {} };
              ({ page: view.page, taskFilter: view.taskFilter, working: view.working, workCard: view.workCard } = prior);
              view.noted = null;
              view.disclosures = prior.open;
              view.aiRoot = prior.aiRoot;
              view.restoreScroll = prior.scroll;
              present(true);
              (menu.querySelector(".content") || document2.scrollingElement).scrollTop = prior.scroll;
              viewState.focus(prior.focus);
            },
            routeButton: function(route, label = routes[route]) {
              return h5("button", { class: "project", "data-route": route }, label);
            },
            answerSlot: function(t4, compact) {
              const label = t4.state === "ask" ? "Open question" : "View agent";
              return h5("button", { "data-a": "workspace", "data-page": "answer", "data-root": t4.root || "", "data-board": t4.key.slice(0, t4.key.lastIndexOf("/")), "data-task": t4.n, "data-revision": t4.revision, ...compact && { class: "compact-action", title: label + " #" + t4.n, "aria-label": label + " #" + t4.n } }, compact ? icon("messages-square") : [icon(t4.state === "ask" ? "messages-square" : "terminal"), label]);
            },
            answerNotices: function(board) {
              return startOperations.answerNotices(board);
            },
            answerToast: function() {
              return startOperations.answerToast();
            },
            tasks: function(p5) {
              return p5.tasks.map((t4) => ({ ...t4, root: p5.root, project: p5.name, boardIdentity: require_task_views().identity(p5), key: p5.issues + "/" + t4.n, url: t4.url || p5.issues + "/" + t4.n }));
            },
            blockProject: function(p5) {
              return { id: p5.root, name: p5.name, board: { url: p5.issues, kind: p5.board_kind }, counts: p5.counts, partial: p5.partial, tasks: tasks(p5) };
            },
            // #1592: the display model marks every repeat registration of a board task `dup`; rows come in model order.
            unique: function(ts) {
              return ts.filter((t4) => !t4.dup).sort((a4, b5) => require_task_views().compare(a4, b5));
            },
            counter: function(v4, kind, label) {
              return h5("span", { class: "counter " + (v4 > 0 ? kind + " filled" : "zero"), "aria-label": label + ": " + v4, title: label }, v4);
            },
            known: function() {
              return !view.S.display.all.partial.length;
            },
            // O1: identity anchors details/actions; title remains a full-width issue link.
            taskRow: function(t4, global2) {
              const p5 = view.S.projects.find((p6) => p6.root === t4.root);
              return h5("div", { class: "task-row", "data-card": t4.key, tabindex: -1, key: t4.key }, raw(TaskqProjectBlocks.task(blockProject(p5), t4, global2, true)), api.syncLine(t4.sync), dashboard?.attention(t4));
            },
            // #1819: a row's shared pending operation; only a failed one offers Retry, which resends the same operation.
            syncLine: function(o4) {
              return syncView.row(o4) || "";
            },
            staleLine: function() {
              return syncView.staleRow(view.S) || "";
            },
            // #1819: the snapshot as received, with the shared pending store laid over it.
            adopt: function(s4) {
              view.S = s4;
            },
            taskOrder: function(a4, b5) {
              return require_task_views().compare(a4, b5);
            },
            startButton: function(rows) {
              const held = rows.filter((t4) => t4.state === "held");
              if (!held.length) return "";
              const label = held.length === 1 && rows.length === 1 ? "Start #" + held[0].n : "Start " + held.length + " held";
              return h5("button", { "data-a": "start", "data-targets": JSON.stringify(held.map(({ key, revision }) => ({ key, revision }))), disabled: held.some(api.startPending), title: label, "aria-label": label }, icon("play"), h5("span", null, label));
            },
            startPending: function(t4) {
              return startOperations.startPending(t4.key.slice(0, t4.key.lastIndexOf("/")), t4);
            },
            startHeld: function(targets) {
              const live = unique(view.S.projects.flatMap(tasks)), rows = targets.map((x5) => live.find((t4) => t4.key === x5.key && t4.revision === x5.revision && t4.state === "held"));
              if (!rows.length || rows.some((t4) => !t4 || api.startPending(t4) || !Number.isSafeInteger(t4.revision) || !view.S.projects.some((p5) => p5.root === t4.root && p5.alive === true && p5.complete === true && p5.tasks_live !== false))) return false;
              for (const t4 of rows) ui.effect("task.start", () => answerSend(t4.key.slice(0, t4.key.lastIndexOf("/")), { id: "tray.start." + t4.n + "." + t4.revision, task: t4.n, revision: t4.revision, action: "start" }), () => {
              });
              return true;
            },
            globalGroup: function(st, ts) {
              ts = ts.filter((t4) => t4.state === st);
              const name = ts[0]?.state_name;
              return ts.length ? h5("details", { class: "group", "data-view-key": "state:" + st, key: "state:" + st }, h5("summary", null, h5("span", { class: color(st) }, name), counter(ts.length, color(st), name)), ts.map((t4) => taskRow(t4, true))) : "";
            },
            problem: function(p5) {
              const e4 = !p5.alive ? p5.age == null ? "no status.json: service not running" : "status.json " + p5.age_text + " old: service not running?" : p5.error ? "service error: " + p5.error : "";
              return e4 ? h5("button", { class: "project alert", "data-a": "log", "data-root": p5.root, title: e4, disabled: !p5.log }, h5("span", null, icon("circle-alert"), p5.name + ": " + e4), h5("span", { class: "meta" }, p5.log ? "Log" : ""), h5("span")) : "";
            },
            modeText: function(p5) {
              return { run: "Configured intake: " + (Array.isArray(p5.focus) ? p5.focus.join(" ") || "off" : "unknown"), paused: "Paused: running tasks finish, no new ones", watch: "Observation only: focus off" }[mode(p5)];
            },
            admission: function(p5) {
              if (!ports.workspace) return api.operationLauncher("admission", p5.root, "Review admission in " + p5.name + "\u2026");
              if (view.pending[p5.root] && !view.pending[p5.root].unknown) return h5("button", { class: "project", disabled: true }, view.pending[p5.root].text + "\u2026");
              if (mode(p5) === "run") return h5("button", { class: "project", "data-a": "pause", "data-root": p5.root }, icon("pause"), "Pause admission in " + p5.name);
              if (p5.saved) return h5("button", { class: "project", "data-a": "resume", "data-root": p5.root }, icon("play"), "Resume admission in " + p5.name);
              if (view.ask !== p5.root) return h5("button", { class: "project", "data-a": "pick", "data-root": p5.root }, "Resume: choose labels\u2026");
              return f4(h5("div", { class: "body meta" }, "No saved focus. Admit tasks with one label:"), h5("div", { class: "options" }, p5.labels.length ? p5.labels.map((l5) => h5("button", { "data-a": "focus", "data-root": p5.root, "data-label": l5 }, l5)) : h5("span", { class: "meta" }, "No labels on open tasks: use taskq service focus")));
            },
            incident: function(p5, top = false) {
              const i4 = p5.incident, local2 = i4 && ["local", "unsupported-auth"].includes(i4.state);
              if (top && (!p5.alive || !local2)) return "";
              const line = !p5.alive ? "Incidents: unknown (" + (p5.age == null ? "no status.json" : "stale status.json") + ")" : p5.incident_line || "Incidents: unknown (no incident status)";
              const text = p5.alive && i4 && (i4.url || i4.path) ? line.replace(" \xB7 " + (i4.url || i4.path), "") : line;
              return f4(
                h5("div", { class: "body " + (top ? "incident-attention" : "meta"), title: line }, (top ? p5.name + ": " : "") + text),
                p5.alive && local2 && h5("div", { class: "options" }, h5("button", { "data-a": "incident-report", "data-root": p5.root, disabled: !i4.path }, "Open report"), h5("button", { "data-a": "incident-manual", "data-root": p5.root }, "Report manually \u2197")),
                p5.alive && i4 && i4.state === "sent" && i4.url && h5("a", { class: "project", href: i4.url, "data-url": i4.url }, "Open incident \u2197")
              );
            },
            incidentNotice: function(p5) {
              return p5.incident_notice ? h5("details", { class: "group incident-notice", "data-view-key": "incident:" + p5.root }, h5("summary", null, "Incident reports: notice ", h5("span", { class: "meta" }, p5.name)), h5("div", { class: "body" }, "Reports stay local until you choose."), h5("div", { class: "body meta" }, "Destination: " + p5.incident_repo + " on GitHub (public). Posting account: unknown here; verify the github.com account used by this project before enabling."), h5("div", { class: "body meta" }, "Only sanitized fault codes, fingerprint, report ID, release, revision, contract, board/runtime/OS classes, source site and evidence codes are sent. No project, task, user, session, token or raw error text."), h5("div", { class: "body meta" }, "Offline or unsupported login: keep a local JSON report. After confirming the destination and posting account, set incident_reports to true in taskq.json to enable, or false to opt out. TASKQ_INCIDENT_REPORTS=off also disables reporting.")) : "";
            },
            service: function(p5, host = true) {
              return h5(
                "details",
                { class: "group service", "data-view-key": "service:" + p5.root },
                api.disclosure("activity", [host ? "Service " : "Project ", h5("span", { class: "meta" }, p5.name + (host ? "" : " \xB7 " + (p5.age == null ? "status unknown" : p5.age_text + " old")))]),
                host && api.hostService(),
                incident(p5),
                p5.log ? h5("button", { class: "project", "data-a": "log", "data-root": p5.root }, icon("file-text"), "Open log") : h5("div", { class: "body meta" }, "Log path unknown"),
                h5("button", { class: "project", "data-a": "folder", "data-root": p5.root }, "Open folder"),
                ports.workspace ? [api.lifecycleControl(p5), api.lifecycleControl(p5, "stop"), api.lifecycleControl(p5, "uninstall")] : api.operationLauncher("service", p5.root, "Review service operations\u2026")
              );
            },
            hostService: function() {
              const s4 = view.S?.service;
              return h5("div", { class: "body meta", role: "status" }, "Host service: " + (s4?.state === "running" ? "running \xB7 PID " + s4.pid + (s4.fresh ? " \xB7 fresh observation" : " \xB7 stale observation") : s4?.state === "exited" ? "recorded process exited; worker outcomes separate" : "unknown") + (s4?.reason ? ": " + s4.reason : ""));
            },
            when: function(t4, tz) {
              return new Date(t4 * 1e3).toLocaleString([], { timeZone: tz === "UTC" ? "UTC" : void 0, dateStyle: "short", timeStyle: "short" }) + " " + (tz === "UTC" ? "UTC" : zone);
            },
            local: function(t4, tz) {
              const d5 = new Date(t4 * 1e3);
              return (tz === "UTC" ? d5.toISOString() : new Date(d5 - d5.getTimezoneOffset() * 6e4).toISOString()).slice(0, 16);
            },
            custom: function() {
              const d5 = view.draft;
              return f4(
                h5("div", { class: "range" }, h5("label", { for: "hs" }, "Start"), h5("input", { id: "hs", type: "datetime-local", "data-draft": "start", value: d5.start }), h5("label", { for: "he" }, "End"), h5("input", { id: "he", type: "datetime-local", "data-draft": "end", value: d5.end }), h5("label", { for: "hz" }, "Time zone"), h5("select", { id: "hz", "data-draft": "tz" }, [["local", "Local (" + zone + ")"], ["UTC", "UTC"]].map(([v4, l5]) => h5("option", { value: v4, selected: d5.tz === v4 }, l5)))),
                d5.error && h5("div", { class: "body alert", role: "alert" }, d5.error),
                h5("button", { class: "project", "data-a": "range" }, "Show [start, end)")
              );
            },
            history: function() {
              const H2 = view.S.history, rows = H2.bins, n3 = rows.length;
              const kinds = ["completed", "new", "ask", "failed", "blocked"], colors = ["work", "new", "ask", "failed", "ready"];
              const values = (b5) => [b5.counts[0], b5.incoming ?? 0, b5.counts[2], b5.counts[1], b5.counts[3]], missing = (b5, k5) => b5.missing.length || k5 === 1 && (b5.incoming_missing?.length ?? 1);
              const identity = (b5) => b5.start + "/" + b5.end, stamp = (t4) => new Date(t4 * 1e3).toLocaleString([], { timeZone: H2.tz === "UTC" ? "UTC" : void 0, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", ...Number.isInteger(t4) ? {} : { fractionalSecondDigits: 3 } }) + " " + (H2.tz === "UTC" ? "UTC" : zone), quantity = (count, missing2) => missing2 ? count ? "\u2265" + count : "? (0 observed)" : String(count);
              const selected = Math.max(0, rows.findIndex((b5) => identity(b5) === view.historyBin));
              const incomplete = rows.some((b5) => b5.missing.length || b5.incoming_missing?.length), tot = kinds.map((_5, k5) => rows.reduce((a4, b5) => a4 + values(b5)[k5], 0)), partial = kinds.map((_5, k5) => rows.some((b5) => missing(b5, k5)));
              const detail = (b5) => "[" + stamp(b5.start) + ", " + stamp(b5.end) + ") \xB7 " + kinds.map((k5, i4) => k5 + " " + quantity(values(b5)[i4], missing(b5, i4))).join(", ") + (b5.ongoing ? " \xB7 ongoing" : "") + (b5.missing.length || b5.incoming_missing?.length ? " \xB7 incomplete: " + [...b5.missing, ...b5.incoming_missing || []].join(", ") : "");
              return h5(
                "details",
                { class: "group hist", "data-view-key": "history" },
                api.disclosure("chart-no-axes-column", ["History ", h5("span", { class: "meta" }, view.S.display.periods.find((x5) => x5.id === view.S.period)?.name)]),
                h5("div", { class: "options periods" }, view.S.display.periods.map(({ id: k5, name: l5 }) => h5("button", { "data-set": "period", "data-v": k5, "aria-pressed": String(view.S.period === k5 || k5 === "custom" && view.draft != null) }, l5))),
                view.draft && custom(),
                h5("div", { class: "body meta" }, when(H2.start, H2.tz) + " \u2013 " + when(H2.end, H2.tz)),
                h5("div", { class: "history-legend body meta" }, kinds.map((k5, i4) => h5("span", null, h5("i", { class: colors[i4] }), k5 + " " + quantity(tot[i4], partial[i4])))),
                h5("div", { class: "body meta history-label" }, "Events \xB7 oldest \u2192 newest"),
                h5(
                  "div",
                  { class: "history-plot" },
                  raw(H2.svg?.replace(/viewBox="0 0 [^"]+"/, 'viewBox="1 10.5 20 11" preserveAspectRatio="none"')),
                  h5("div", { class: "history-targets", role: "group", "aria-label": "Event history; six equal bins" }, rows.map((b5, i4) => h5("button", { "data-history-bin": identity(b5), "aria-label": detail(b5), "aria-pressed": String(i4 === selected), title: detail(b5) })))
                ),
                h5("div", { class: "body meta history-detail", role: "status" }, rows[selected] ? when(rows[selected].start, H2.tz) + " \u2013 " + when(rows[selected].end, H2.tz) + " \xB7 " + kinds.map((k5, i4) => k5 + " " + quantity(values(rows[selected])[i4], missing(rows[selected], i4))).join(" \xB7 ") + (rows[selected].ongoing ? " \xB7 ongoing" : "") : "History unavailable"),
                incomplete && h5("div", { class: "body meta" }, "Incomplete \xB7 observed lower bounds"),
                h5(
                  "details",
                  { "data-view-key": "history-source" },
                  api.disclosure("info", "Source and exact interval"),
                  h5("div", { class: "body meta history-exact" }, rows[selected] ? detail(rows[selected]) : "History unavailable"),
                  h5("div", { class: "body meta history-sources" }, "Sources: " + (view.S.projects.map((p5) => p5.name + (p5.since != null ? " events since " + stamp(p5.since) : " unknown: " + (p5.events_error || "no history")) + (p5.incoming?.schema === 1 ? " \xB7 New coverage " + (p5.incoming.start == null ? "unknown" : stamp(p5.incoming.start) + " \u2013 " + stamp(p5.incoming.end)) : " \xB7 New not collected")).join("; ") || "none registered")),
                  h5("div", { class: "body meta" }, "Counts are events, not current tasks. Peak " + (H2.peak ?? "unknown") + " events (Completed + available New + Ask + Failed). Counts are not unique-task productivity. Collection start does not prove uninterrupted collection.")
                )
              );
            },
            update: function() {
              if (!ports.workspace) return api.operationLauncher("update", null, "Review updates\u2026");
              const up = view.S.projects.find((p5) => p5.update && p5.update !== p5.version);
              if (view.updating && view.S.projects.some((p5) => p5.root === view.updating.root && p5.alive === true && p5.version === view.updating.v)) return h5("div", { class: "row meta" }, "Updated to " + view.updating.v + ": read from status.json");
              if (view.updating && view.S.now - view.updating.at < 300) return h5("button", { class: "project", disabled: true }, "Updating to " + view.updating.v + "\u2026");
              return f4(view.updating && h5("div", { class: "row alert" }, "Update to " + view.updating.v + " not confirmed: see the log"), !view.updating && up && h5("button", { class: "project", "data-a": "update", "data-root": up.root, "data-v": up.update }, "Update now to " + up.update));
            },
            ai: function() {
              return h5("button", { class: "project", "data-a": "workspace", "data-page": "conversations", "data-fresh": "true", "data-root": view.aiRoot || null }, "Ask AI \xB7 new session\u2026");
            },
            addProject: function() {
              if (!ports.workspace) return api.operationLauncher("setup", null, "Add project in workspace\u2026");
              const A4 = view.S.add;
              if (!A4) return h5("button", { class: "project", "data-a": "add" }, "Add project\u2026");
              if (A4.state === "picking") return f4(h5("div", { class: "row meta" }, "Choose a folder\u2026"), h5("button", { class: "project", "data-a": "add-cancel" }, "Hide picker result"));
              if (A4.state === "picking dismissed") return h5("div", { class: "body meta" }, "Folder selection is still being processed; its result will be ignored.");
              if (A4.state === "installing dismissed") return h5("div", { class: "body meta" }, "Installation was submitted; its result is hidden.");
              if (A4.state === "pick failed" || A4.state === "add failed") return f4(h5("div", { class: "body alert", role: "alert" }, (A4.state === "pick failed" ? "Folder picker failed" : "Add project unavailable") + (A4.reason ? ": " + A4.reason : "")), h5("button", { class: "project", "data-a": "add-cancel" }, "Done"));
              const failed = A4.state === "install failed", known2 = view.S.projects.some((p5) => p5.root === A4.path);
              const out = [h5("div", { class: "body" }, h5("div", { class: "meta" }, "Folder: " + A4.path), h5("div", { role: "status", class: A4.kind === "old" || failed ? "alert" : "" }, (failed ? "Install failed" : addKinds[A4.kind]) + (A4.reason ? ": " + A4.reason : "")))];
              if (A4.kind === "old") out.push(h5("a", { class: "project", href: cutover, "data-url": cutover }, h5("span", null, "Repair or cutover: taskq.md \xA7 9 \u2197"), h5("span"), h5("span")));
              if (A4.kind === "taskq") {
                if (A4.state === "installed") out.push(h5("div", { class: "row work", role: "status" }, "Installed: read back from this host's projects"));
                else if (A4.state === "installing") out.push(h5("button", { class: "project", disabled: true }, "Installing\u2026"));
                else if (known2) out.push(h5("div", { class: "body meta" }, "Already in this menu"));
                else if (view.addConfirm) out.push(h5("div", { class: "body", role: "alert" }, "Register " + A4.path + " and install its service? It starts at logon."), h5("div", { class: "options" }, h5("button", { "data-a": "add-install" }, "Confirm"), h5("button", { "data-a": "add-back" }, "Cancel")));
                else if (!failed) out.push(h5("button", { class: "project", "data-a": "add-ask" }, "Install service\u2026"));
              }
              if (A4.kind === "new" || A4.kind === "plain") out.push(h5("button", { class: "project", "data-a": "add-setup" }, "Set up with AI\u2026"));
              return f4(out, h5("button", { class: "project", "data-a": "add-cancel" }, A4.state === "installing" ? "Hide result; install continues" : A4.state === "installed" ? "Done" : "Cancel"));
            },
            bulk: function(run, held) {
              if (!ports.workspace) return run.length || held.length ? api.operationLauncher("admission", null, "Review admission\u2026") : "";
              const pausing = run.length > 0;
              const state = (p5) => ({ run: pausing ? "pause" : "admitting, unchanged", paused: pausing ? "paused, unchanged" : "resume", watch: "observation, unchanged" })[mode(p5)];
              if (view.confirming) return f4(h5("div", { class: "body", role: "alert" }, pausing ? "Pause admission? Running tasks finish." : "Resume admission?", h5("br"), lines(view.S.projects.map((p5) => p5.name + ": " + state(p5)))), h5("div", { class: "options" }, h5("button", { "data-a": pausing ? "pause-all" : "resume-all" }, "Confirm"), h5("button", { "data-a": "cancel" }, "Cancel")));
              return run.length ? h5("button", { class: "project", "data-a": "confirm" }, "Pause all\u2026") : held.length ? h5("button", { class: "project", "data-a": "confirm" }, "Resume all\u2026") : "";
            },
            views: function() {
              return h5("div", { class: "options views" }, [["icon", "Icon"], ["full", "Summary"]].map(([v4, l5]) => h5("button", { "data-set": "view", "data-v": v4, "aria-pressed": String(view.S.view === v4) }, v4 === "full" ? h5("span", { class: "summary-preview" }, raw(view.S.icons?.full)) : view.S.icons?.icon && h5("img", { alt: "", src: view.S.icons.icon }), l5)));
            },
            appearance: function() {
              const reduced = window2.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches, background = view.S.opacity_mode === "background";
              return f4(
                api.heading("Text size \xB7 " + view.S.font + " px", "type"),
                h5("div", { class: "body meta" }, "Theme follows system appearance."),
                h5("div", { class: "options" }, [14, 16, 18, 20].map((n3) => h5("button", { "data-set": "font", "data-v": n3, "aria-pressed": String(n3 === view.S.font), style: "font-size:" + n3 + "px" }, n3))),
                api.heading("Tray icon", "panels-top-left"),
                views(),
                api.heading("Window transparency", "sliders-horizontal"),
                h5("div", { class: "options" }, [["whole", "Whole window"], ["background", "Background only"]].map(([v4, label]) => h5("button", { "data-set": "opacity_mode", "data-v": v4, "aria-pressed": String((view.S.opacity_mode || "whole") === v4) }, label))),
                background ? f4(
                  h5("div", { class: "body opacity-setting" }, h5("label", { for: "background-opacity" }, "Background opacity \xB7 " + (view.S.background_opacity ?? 100) + "%"), h5("input", { id: "background-opacity", type: "range", min: 0, max: 100, step: 1, value: view.S.background_opacity ?? 100, "data-opacity": "background_opacity", "aria-valuetext": (view.S.background_opacity ?? 100) + "% opaque" })),
                  h5("div", { class: "body meta" }, "The task filter background stays opaque. Other fills fade; text and controls stay opaque. At 0%, the desktop shows through.")
                ) : f4(
                  h5("div", { class: "body opacity-setting" }, h5("label", { for: "menu-opacity" }, "Menu opacity \xB7 " + (view.S.opacity ?? 100) + "%"), h5("button", { "data-set": "opacity", "data-v": 100, "aria-pressed": String((view.S.opacity ?? 100) === 100) }, "Off"), h5("input", { id: "menu-opacity", type: "range", min: 60, max: 100, step: 1, value: view.S.opacity ?? 100, "data-opacity": "opacity", "aria-valuetext": (view.S.opacity ?? 100) === 100 ? "Off \xB7 100% opaque" : view.S.opacity + "% opaque" })),
                  h5("div", { class: "body meta" }, "Whole window: 60\u2013100% opaque, including text and controls.")
                ),
                api.heading("Window", "maximize-2"),
                api.windowSettings(),
                reduced && h5("div", { class: "body meta", role: "status" }, "Reduced motion: growth previews show a still outline and arrow.")
              );
            },
            agentq: function(settings = false) {
              const A4 = view.S.agentq;
              if (!A4) return "";
              if (A4.kind === "running") return settings ? h5("div", { class: "body", role: "status" }, "agentq running \xB7 " + (A4.version || "version unknown")) : "";
              if (!settings && A4.kind === "shown" && A4.dismissed && !view.agentqError) return "";
              const rows = Array.isArray(A4.runtimes) ? A4.runtimes : [], runtime = view.agentqRuntime || rows.find((r4) => r4.eligible)?.runtime, selected = rows.find((r4) => r4.runtime === runtime), targets = A4.targets?.[runtime] || [];
              if (A4.kind === "stopped" || A4.kind === "unknown") return h5(
                "section",
                { class: "group agentq", "aria-label": "agentq " + A4.kind },
                view.agentqError && h5("div", { class: "body alert", role: "alert" }, view.agentqError),
                h5("div", { class: "row" }, A4.kind === "stopped" ? "agentq off or unreachable" : "agentq status unknown"),
                h5("div", { class: "body meta" }, A4.host + " \xB7 " + A4.platform),
                A4.reason && h5("div", { class: "body", role: "status" }, A4.reason),
                h5("div", { class: "body meta" }, "taskq runs without agentq. Start or repair it from agentq itself; nothing is installed or changed here."),
                h5("div", { class: "options" }, h5("button", { "data-a": "agentq-check" }, "Check again"))
              );
              const session = A4.setup?.url && /^(https:\/\/claude\.ai\/code\/session_[A-Za-z0-9]+|codex:\/\/threads\/[A-Za-z0-9-]+)$/.test(A4.setup.url) ? h5("a", { href: "#", "data-url": A4.setup.url }, "Open session") : null;
              return h5(
                "section",
                { class: "group agentq", "aria-label": A4.kind === "shown" ? "agentq is not installed" : "agentq " + A4.kind },
                view.agentqError && h5("div", { class: "body alert", role: "alert" }, view.agentqError),
                h5("div", { class: "row" }, A4.kind === "shown" ? settings ? "agentq not installed" : "Save on repeated context" : A4.kind === "setting-up" ? "Setting up agentq\u2026" : A4.kind === "stopped" ? "agentq installed \xB7 service stopped" : "agentq " + (A4.kind === "unknown" ? "status unknown" : A4.kind)),
                h5("div", { class: "body meta" }, A4.host + " \xB7 " + A4.platform),
                A4.reason && h5("div", { class: "body", role: "status" }, A4.reason),
                A4.setup && h5("div", { class: "body", role: "status" }, "Setup \xB7 " + A4.setup.kind, A4.setup.reason && " \xB7 " + A4.setup.reason, A4.setup.next && " \xB7 " + A4.setup.next, " ", session),
                A4.kind === "shown" && h5("div", { class: "body" }, "agentq can reduce repeated context and preserve cache reuse, which may lower model costs. Savings depend on your client and workload; no saving is guaranteed."),
                h5("div", { class: "options" }, rows.map((r4) => h5("button", { "data-a": "agentq-runtime", "data-runtime": r4.runtime, disabled: !r4.eligible, "aria-pressed": String(runtime === r4.runtime) }, r4.client))),
                rows.filter((r4) => !r4.eligible).map((r4) => h5("div", { class: "body meta" }, r4.client + " \xB7 " + (r4.reason || "unknown"))),
                selected && h5("div", { class: "body" }, "Set up agentq on this host for " + selected.client + " using " + runtime + ". This installs its verified release, engines and host service, and connects this client\u2019s supported configuration, hooks and MCP. Existing sessions keep running. New sessions may be needed to verify the connection."),
                selected && h5("div", { class: "body meta" }, "Approved targets: " + targets.join(", ")),
                h5("div", { class: "options" }, h5("button", { "data-a": "agentq-setup", disabled: !selected?.eligible || targets.length !== 2 }, settings ? "Set up agentq\u2026" : "OK \u2014 set up agentq"), A4.kind === "shown" && !A4.dismissed && h5("button", { "data-a": "agentq-dismiss" }, "Not now"), h5("button", { "data-a": "agentq-check" }, "Check again")),
                !selected?.eligible && h5("div", { class: "body meta" }, "Use an existing client: Install agentq from https://github.com/taskqorg/agentq and set it up for this client. No runtime or permissions are changed here.")
              );
            },
            agents: function() {
              const A4 = view.S.agents, local2 = view.S.projects.filter((p5) => p5.host === view.S.host), remote = [...new Set(view.S.projects.flatMap((p5) => [p5.host, ...(Array.isArray(p5.hosts) ? p5.hosts : []).map((h6) => h6?.name), ...(p5.tasks || []).flatMap((t4) => (t4.executions || []).map((x5) => x5.host))]).filter((x5) => x5 && x5 !== view.S.host))].sort();
              const prior = (r4) => r4.state === "signed-in" ? A4.fresh ? "Signed in" : "Signed in at " + when(A4.at) : signState[r4.state] || "Unknown";
              const label = (r4) => A4.checking ? "Checking" : prior(r4), checked = A4?.at == null ? "no earlier check" : "checked " + A4.age + " ago";
              const rows = A4?.rows.length ? A4.rows.map((r4) => h5(
                "details",
                { class: "group agent-signin", "data-view-key": "agent:" + r4.runtime },
                api.disclosure(r4.runtime === "claude" ? "claude" : r4.runtime === "antigravity" ? "bot" : "openai", [runtimeName(r4.runtime) + " " + (r4.version || "version unknown") + " \xB7 ", h5("span", { class: !A4.checking && signAlert.includes(r4.state) ? "alert" : "" }, label(r4)), h5("span", { class: "meta" }, " \xB7 " + (r4.account || "account unknown"))]),
                h5("div", { class: "body meta" }, lines([A4.checking && "Last result: " + prior(r4), r4.method && "Method: " + r4.method, r4.expires ? "Expires " + when(r4.expires) : "Expiry unknown", r4.refreshed && "Last refresh " + r4.refreshed + "; not an expiry", r4.reason && "Reason: " + r4.reason, checked].filter(Boolean))),
                r4.next && h5("div", { class: "body" }, "Sign in: " + r4.next + ". A check never signs in.")
              )) : [h5("div", { class: "body meta" }, A4?.checking ? "Checking: no earlier result" : "Unknown: no status.json has agents")];
              return f4(
                api.heading("Clients on this computer", "bot"),
                h5("div", { class: "row" }, h5("span", null, h5("strong", null, view.S.host || "This host"), " \xB7 sign-in " + (A4 ? (A4.checking ? "checking now \xB7 " : "") + checked : "unknown: no service probe yet"))),
                h5("div", { class: "body meta" }, "Tray " + (view.S.version || "version unknown") + " \xB7 service " + (grouped(local2.map((p5) => p5.version || "version unknown")) || "none on this host")),
                rows,
                // #1702 A3: a remote host's only source here is its board heartbeat (freshness); client, sign-in, account and access read unknown with the missing source
                remote.length > 0 && api.heading("Other computers", "network"),
                remote.map((n3) => {
                  const beat = view.S.projects.flatMap((p5) => Array.isArray(p5.hosts) ? p5.hosts : []).find((x5) => x5?.name === n3);
                  return h5(
                    "details",
                    { class: "group remote-agents", "data-view-key": "agents:" + n3 },
                    api.disclosure("network", n3 + " \xB7 " + (beat?.age_text ? "heartbeat " + beat.age_text + " old" : beat ? "heartbeat age unknown" : "heartbeat unknown: no host row") + " \xB7 clients unknown"),
                    ["claude", "codex", "antigravity", "codex-app"].map((rt) => h5("div", { class: "body meta" }, runtimeName(rt) + " \xB7 unknown: " + n3 + " heartbeat publishes no client version, sign-in, account or access")),
                    h5("div", { class: "body meta" }, "Source: board heartbeat only; no runtime-client or access probe of " + n3 + " reaches this host.")
                  );
                }),
                api.heading("Project access", "shield"),
                api.accessFacts(),
                h5("div", { class: "body meta" }, "Editing is not available here: the operator changes grants with `taskq service permissions --runtime <runtime> --mode <mode>`."),
                view.S.agentq && api.heading("AgentQ", "bot"),
                api.agentq(true)
              );
            },
            // R29/R32: per project on this host, effective or refused access per runtime, or the one reason it is unknown
            accessFacts: function() {
              const local2 = view.S.projects.filter((p5) => p5.host === view.S.host), runtimes = [...new Set(local2.flatMap((p5) => (p5.access || []).map((a4) => a4.runtime)))];
              return f4(
                local2.map((p5) => h5(
                  "section",
                  { class: "access-project", "data-access-project": p5.root },
                  h5("div", { class: "body" }, p5.name),
                  Array.isArray(p5.access) && p5.access.length ? h5("div", { class: "access-grid", style: { "--access-columns": Math.max(1, runtimes.length) } }, runtimes.map((rt) => {
                    const a4 = p5.access.find((a5) => a5.runtime === rt), state = !a4 ? "unknown" : a4.refused ? "refused" : { "workspace-write": "Workspace write", workspace: "Workspace", full: "Full", "read-only": "Read only" }[a4.effective] || a4.effective || "unknown";
                    return h5(
                      "details",
                      { class: "access", "data-view-key": "access:" + p5.root + ":" + rt },
                      api.disclosure("shield", runtimeName(rt) + " \xB7 " + (p5.alive !== true ? "at capture: " : "") + state),
                      h5("div", { class: "body meta" }, !a4 ? "Unknown: no runtime access in status.json" : runtimeName(rt) + ": " + (a4.refused ? "refused: " + a4.refused : "requested " + (a4.requested || "unknown") + " \xB7 saved grant " + (a4.grant || "none") + " \xB7 applied " + (a4.effective || "unknown") + (a4.enforcement ? " \xB7 " + a4.enforcement : "")))
                    );
                  })) : h5("div", { class: "body meta" }, "Access unknown for " + p5.name + ": no access in status.json"),
                  h5("div", { class: "body meta" }, "Applied at the last service pass \xB7 " + (p5.age == null ? "age unknown" : p5.age_text + " old"))
                )),
                !local2.length && h5("div", { class: "body meta" }, "Access unknown: no project on this host")
              );
            },
            about: function() {
              const S3 = view.S, up = S3.projects.find((p5) => p5.update && p5.update !== p5.version), versions = /* @__PURE__ */ new Map();
              for (const p5 of S3.projects) versions.set(p5.version || "version unknown", [...versions.get(p5.version || "version unknown") || [], p5]);
              return f4(
                api.heading("Versions and updates", "info"),
                h5("div", { class: "body" }, "Tray installed " + (S3.version || "version unknown") + " \xB7 " + S3.host),
                [...versions].map(([v4, ps]) => h5("details", { class: "group", "data-view-key": "version:" + v4 }, api.disclosure("info", ["Service " + v4, h5("span", { class: "meta" }, " \xB7 " + ps.length + " project" + (ps.length === 1 ? "" : "s"))]), ps.map((p5) => h5("div", { class: "body meta" }, p5.name + " \xB7 " + (p5.age == null ? "status unknown" : p5.age_text + " old"))))),
                h5("div", { class: "body meta", role: "status" }, up ? "Update available: " + up.update : S3.projects.some((p5) => p5.alive === true) ? "No newer version reported" : "Update check unknown"),
                update()
              );
            },
            setup: function() {
              const s4 = view.S.setup;
              if (!s4 || view.S.agentq?.setup?.id && view.S.agentq.setup.id === s4.id) return "";
              if (!ports.workspace) return f4(h5("div", { class: "body meta" }, "Setup \xB7 " + (s4.kind || "unknown")), api.operationLauncher("setup", null, "Review setup status\u2026"));
              const link = s4.url && /^(https:\/\/claude\.ai\/code\/session_[A-Za-z0-9]+|codex:\/\/threads\/[A-Za-z0-9-]+)$/.test(s4.url) && [" ", h5("a", { href: "#", "data-url": s4.url }, "Open session")];
              return h5("div", { class: "group" }, h5("div", { class: "row" }, "Setup \xB7 " + s4.kind, link), s4.reason && h5("div", { class: "body meta" }, s4.reason), h5("div", { class: "body" }, s4.next || "Setup state unknown; run `taskq service setup`"));
            },
            attention: function() {
              const all = unique(view.S.projects.flatMap(tasks));
              const parts = [api.agentq(), setup(), ...view.S.projects.flatMap((p5) => [problem(p5), incident(p5, true), incidentNotice(p5), api.managerView(p5)]), globalGroup("ask", all), globalGroup("failed", all), answerNotices()].filter(Boolean);
              return parts.length ? f4(parts) : "";
            },
            // R6/M3.7: the service's manager view (status.json `report`, same text as `taskq status`); anomaly rows lead, the full view folds
            managerView: function(p5) {
              const r4 = typeof p5.report === "string" ? p5.report : "";
              if (!r4) return "";
              const plain = (l5) => l5.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1"), rows = (r4.split("Anomalies:\n")[1] || "").split("\n\n")[0].split("\n").filter((l5) => l5.startsWith("- "));
              return h5(
                "details",
                { class: "manager-view", "data-view-key": "manager-view:" + p5.root },
                h5("summary", null, p5.name + ": manager view" + (rows.length ? " \xB7 " + rows.length + (rows.length === 1 ? " anomaly" : " anomalies") : "")),
                rows.map((l5) => h5("div", { class: "body incident-attention" }, plain(l5.slice(2)))),
                h5("pre", { class: "manager-report", style: "white-space:pre-wrap;overflow-wrap:anywhere;margin:0;font:inherit" }, plain(r4))
              );
            },
            projects: function() {
              return f4(api.heading("Registered projects", "folder"), view.S.projects.length ? view.S.projects.map((p5) => h5(
                "section",
                { class: "settings-project" },
                h5("div", { class: "project-shortcuts" }, h5("button", { class: "project-overview", "data-route": p5.root, title: "Open " + p5.name + " project overview" }, p5.name), h5(
                  "div",
                  { class: "project-actions" },
                  p5.issues ? h5("a", { class: "compact-action", href: p5.issues, "data-url": p5.issues, title: "Open " + p5.name + " board", "aria-label": "Open " + p5.name + " board" }, icon(p5.board_kind)) : button("link", "Board unavailable: registered board missing", { disabled: true }),
                  button("folder", p5.root ? "Open " + p5.name + " project folder on this computer" : "Project folder unavailable: registered root missing", { "data-a": "folder", "data-root": p5.root, disabled: !p5.root, class: "compact-action" })
                )),
                (p5.alive !== true || p5.error || mode(p5) !== "run") && h5("div", { class: "body meta" }, p5.error || (p5.alive !== true ? "Status unknown: " + (p5.age == null ? "no status.json" : p5.age_text + " old") : modeText(p5) || "Intake unknown")),
                h5("details", { "data-view-key": "project-settings:" + p5.root }, api.disclosure("focus", "Intake details"), h5("div", { class: "body meta" }, modeText(p5) || "Configured intake unknown"), h5("div", { class: "body meta" }, "You asked: " + (p5.request || "not recorded")), h5("div", { class: "body meta" }, "Readback: " + (p5.age == null ? "status unknown" : p5.age_text + " old")), api.operationLauncher("admission", p5.root, "Review project admission\u2026"))
              )) : h5("div", { class: "body meta" }, "No projects yet"), addProject());
            },
            controls: function() {
              const run = view.S.projects.filter((p5) => mode(p5) === "run"), held = view.S.projects.filter((p5) => mode(p5) === "paused");
              return f4(workEntry(), held.length > 0 && run.length > 0 && h5("div", { class: "row meta" }, "Paused " + held.length + " of " + (held.length + run.length) + ": " + held.map((p5) => p5.name).join(", ")), bulk(run, held));
            },
            runningAgents: function(compact = false) {
              const hosts = /* @__PURE__ */ new Map(), all = view.S.display.all.counts;
              let partial = false;
              const sources = /* @__PURE__ */ new Set(), seen = /* @__PURE__ */ new Map();
              const add = (name, why) => {
                if (!hosts.has(name)) hosts.set(name, { name, age: null, age_text: null, live: false, reasons: /* @__PURE__ */ new Set(), counts: /* @__PURE__ */ new Map() });
                const host = hosts.get(name);
                if (why) host.reasons.add(why);
                return host;
              };
              for (const p5 of view.S.projects) {
                const elapsed = Number.isFinite(p5.age) && p5.age >= 0 && Number.isFinite(p5.tasks_age_seconds) && p5.tasks_age_seconds >= 0 ? p5.age + p5.tasks_age_seconds : null;
                const fresh = p5.complete === true && p5.tasks_live === true && Number.isFinite(p5.observed);
                const read = Array.isArray(p5.tasks) && p5.tasks.every((t4) => t4?.executions == null || Array.isArray(t4.executions));
                const stale = fresh && read ? null : (!read ? "missing host/task data" : !p5.complete ? "partial board snapshot" : elapsed === null ? "missing task observation" : "stale task observation") + " (" + (p5.name || p5.root) + ")" + (elapsed === null ? " \xB7 age unknown" : " \xB7 " + Math.round(elapsed) + "s old");
                partial ||= !fresh || !Array.isArray(p5.hosts) || !Array.isArray(p5.tasks);
                if (stale) sources.add(stale);
                if (!Array.isArray(p5.hosts)) sources.add((p5.name || "Project") + ": host observations missing");
                for (const h6 of Array.isArray(p5.hosts) ? p5.hosts : []) {
                  if (!h6 || typeof h6.name !== "string" || !h6.name) {
                    partial = true;
                    continue;
                  }
                  const age = Number.isFinite(h6.age) ? h6.age : null, host = add(h6.name, stale);
                  host.beat = true;
                  host.beatLive ||= h6.live === true;
                  host.live ||= h6.live === true && !stale;
                  if (age !== null && (host.age === null || age < host.age)) Object.assign(host, { age, age_text: h6.age_text });
                }
                for (const t4 of Array.isArray(p5.tasks) ? p5.tasks : []) {
                  if (t4.executions != null && !Array.isArray(t4.executions)) partial = true;
                  for (const x5 of Array.isArray(t4.executions) ? t4.executions : []) {
                    if (!x5 || typeof x5.host !== "string") {
                      partial = true;
                      sources.add((p5.name || "Project") + ": invalid execution observation; host unknown");
                      continue;
                    }
                    const h6 = add(x5.host || "Unknown host", stale);
                    if (fresh && ["ended", "waiting"].includes(x5.state)) continue;
                    if (!fresh || x5.live !== true || x5.state !== "running" || !["worker", "reviewer"].includes(x5.role) || typeof x5.runtime !== "string" || !x5.runtime || !Number.isSafeInteger(x5.id) || x5.id <= 0) {
                      h6.reasons.add("run state unknown; see service log");
                      continue;
                    }
                    if (!h6.counts.has(x5.runtime)) h6.counts.set(x5.runtime, { worker: 0, reviewer: 0 });
                    const identity = JSON.stringify([t4.key, x5.id]), prior = seen.get(identity);
                    if (prior) {
                      if (prior.host !== x5.host || prior.runtime !== x5.runtime || prior.role !== x5.role) {
                        h6.reasons.add("conflicting execution observations");
                        hosts.get(prior.host).reasons.add("conflicting execution observations");
                      }
                      continue;
                    }
                    seen.set(identity, x5);
                    const counts = h6.counts.get(x5.runtime) || { worker: 0, reviewer: 0 };
                    counts[x5.role]++;
                    h6.counts.set(x5.runtime, counts);
                  }
                }
              }
              if (view.S.host && !hosts.has(view.S.host)) add(view.S.host, "no host observation");
              const local2 = view.S.projects.filter((p5) => p5.host === view.S.host), latest = local2.filter((p5) => p5.capacity).sort((a4, b5) => a4.age - b5.age)[0];
              const cap = latest?.capacity?.host_runtime_used, capFresh = latest?.alive === true, load = latest?.capacity;
              const occupied = cap && typeof cap === "object" && !Array.isArray(cap) && Object.values(cap).every((n3) => Number.isInteger(n3) && n3 >= 0);
              const facts = [capFresh && Number.isFinite(load?.cpu_percent) && Number.isFinite(load?.memory_available_percent) && latest.sample_age_text ? "CPU " + Math.round(load.cpu_percent) + "% busy \xB7 memory " + Math.round(load.memory_available_percent) + "% available \xB7 sampled " + latest.sample_age_text + " ago" : "CPU and memory: no sample; sampled only while the load gate is on"];
              if (capFresh && occupied) {
                facts.push("Occupied slots: " + (Object.entries(cap).map(([rt, n3]) => rt + " " + n3).join(" \xB7 ") || "0") + " \xB7 " + latest.age_text + " old");
                const reserved = /* @__PURE__ */ new Map();
                let known3 = local2.length > 0;
                for (const p5 of local2) {
                  if (p5.alive !== true || p5.complete !== true || p5.tasks_live !== true || !Array.isArray(p5.capacity?.reservations)) {
                    known3 = false;
                    continue;
                  }
                  for (const r4 of p5.capacity.reservations) {
                    if (!r4 || typeof r4.rt !== "string" || !r4.rt || !["held", "requested"].includes(r4.state)) {
                      known3 = false;
                      continue;
                    }
                    const running = (p5.tasks || []).some((t4) => t4.n === r4.task && (Array.isArray(t4.executions) ? t4.executions : []).some((x5) => x5 && x5.attempt === r4.attempt && x5.host === view.S.host && x5.runtime === r4.rt && x5.state === "running" && x5.live === true));
                    if (!running) reserved.set(p5.issues + "/" + r4.task + "/" + r4.attempt, r4.rt);
                  }
                }
                const counts = /* @__PURE__ */ new Map();
                for (const rt of reserved.values()) counts.set(rt, (counts.get(rt) || 0) + 1);
                facts.push("Reservations: " + (known3 ? [...counts].map(([rt, n3]) => rt + " " + n3).join(" \xB7 ") || "none" : "unknown"));
              } else facts.push("Capacity unknown");
              const alerts = (view.S.agents?.rows || []).filter((r4) => signAlert.includes(r4.state) && r4.state !== "check-failed");
              const totals = (host) => {
                const t4 = /* @__PURE__ */ new Map([["Claude", 0], ["Codex", 0], ["Antigravity", 0]]);
                for (const [rt, c4] of host.counts) t4.set(family(rt), (t4.get(family(rt)) || 0) + c4.worker + c4.reviewer);
                return t4;
              };
              for (const host of hosts.values()) if (!host.live) host.reasons.add(!host.beat ? "host observation missing" : host.beatLive ? "current execution data unavailable" : host.age === null ? "heartbeat freshness unknown" : "stale heartbeat; current runs unknown");
              const known2 = (host) => host.live && !host.reasons.has("conflicting execution observations");
              const sum = (counts) => [...counts.values()].reduce((n3, x5) => n3 + x5, 0);
              const value = (n3, known3, partial2) => !known3 || partial2 && !n3 ? "?" : partial2 ? "\u2265" + n3 : String(n3);
              const mark = (rt) => rt === "Codex" ? "openai" : rt === "Claude" ? "claude" : "bot";
              const runtime = (rt, n3, known3, partial2) => {
                const shown = value(n3, known3, partial2), label = rt + ": " + (shown === "?" ? "unknown" : partial2 ? "at least " + n3 + "; additional agents unknown" : n3);
                return h5("span", { class: "host-runtime", "data-runtime": rt, title: label, "aria-label": label }, icon(mark(rt)), !["Codex", "Claude"].includes(rt) && h5("span", null, rt), h5("span", { class: "host-number" }, shown));
              };
              const out = [], sorted = [...hosts.values()].sort((a4, b5) => a4.name.localeCompare(b5.name));
              const numberWidth = Math.max(3, ...sorted.flatMap((host) => {
                const t4 = totals(host);
                return [sum(t4), ...t4.values()].map((n3) => value(n3, known2(host), host.reasons.size > 0).length);
              })) + "ch";
              for (const host of sorted) {
                const unknown = !known2(host), here = host.name === view.S.host, age = host.age !== null && host.age >= 0 && host.age_text ? " \xB7 " + host.age_text + " old" : " \xB7 age unknown", why = [...host.reasons].join("; "), t4 = totals(host), partial2 = host.reasons.size > 0, total = value(sum(t4), !unknown, partial2), warnings = here ? alerts.map((r4) => family(r4.runtime) + " " + signState[r4.state].toLowerCase()) : [];
                const runtimes = [...host.counts].sort(([a4], [b5]) => a4.localeCompare(b5));
                out.push(h5(
                  "details",
                  { class: "host-line", "data-view-key": "host:" + host.name, key: "host:" + host.name },
                  h5(
                    "summary",
                    { class: "host-breakdown", title: "Heartbeat" + age },
                    h5("strong", { class: "host-name" }, host.name),
                    h5("span", { class: "host-runtime-split" }, ["Codex", "Claude", "Antigravity", ...[...t4.keys()].filter((rt) => !["Codex", "Claude", "Antigravity"].includes(rt))].map((rt) => runtime(rt, t4.get(rt) || 0, !unknown, partial2))),
                    h5("span", { class: "host-row-total", "aria-label": "Total running agents: " + total }, h5("span", { class: "host-number" }, total)),
                    (unknown || partial2 || warnings.length > 0) && h5("span", { class: "host-observation" }, unknown ? "Status unknown" + age : partial2 ? "Partial observation" + age : "", why && h5("span", null, why), warnings.length > 0 && h5("span", { class: "host-warning" }, "Sign-in: " + warnings.join(" \xB7 ")))
                  ),
                  h5("div", { class: "body meta" }, "Heartbeat" + age),
                  unknown ? h5("div", { class: "body meta" }, "Unknown: " + (why || "host observation missing or stale")) : why && h5("div", { class: "body meta" }, "Partial: " + why),
                  !unknown && (runtimes.length ? h5("div", { class: "body" }, runtimes.map(([rt, c4]) => h5("div", null, rt + ": " + c4.worker + " worker" + (c4.worker === 1 ? "" : "s") + " \xB7 " + c4.reviewer + " reviewer" + (c4.reviewer === 1 ? "" : "s")))) : h5("div", { class: "body meta" }, partial2 ? "No confirmed running agents; additional agents unknown" : "No running agents")),
                  here && api.resourceMeters(latest),
                  h5("details", { "data-view-key": "host-facts:" + host.name }, api.disclosure("sliders-horizontal", "Resource details"), (here ? facts : ["CPU and memory: not collected for remote hosts", "Slots and reservations: unknown for remote hosts"]).map((t5) => h5("div", { class: "body meta" }, t5)))
                ));
              }
              const strip = sorted.filter((host) => host.beat), names = strip.map((host) => host.name);
              const short = new Map(names.map((n3) => {
                let k5 = 3;
                while (k5 < n3.length && names.some((o4) => o4 !== n3 && o4.slice(0, k5) === n3.slice(0, k5))) k5++;
                return [n3, n3.slice(0, k5)];
              }));
              const warn = (host) => host.name === view.S.host ? alerts.map((r4) => family(r4.runtime) + " " + signState[r4.state].toLowerCase()) : [];
              const text = (host) => {
                const age = host.age !== null && host.age >= 0 ? host.age_text : "", busy = [...totals(host)].filter(([, n3]) => n3 > 0);
                return [host.name, ...!known2(host) ? ["runs unknown", ...age ? [age + " old"] : [], ...host.reasons.size ? [[...host.reasons].join("; ")] : []] : [...busy.length ? busy.map(([k5, n3]) => k5 + " " + n3) : [host.reasons.size ? "no confirmed running agents" : "idle"], ...host.reasons.size ? ["partial: " + [...host.reasons].join("; ")] : []], ...warn(host)].join(" \xB7 ");
              };
              const overall = /* @__PURE__ */ new Map([["Codex", 0], ["Claude", 0], ["Antigravity", 0]]);
              let anyKnown = false, uncertain = partial || !sorted.length;
              for (const host of sorted) {
                const t4 = totals(host);
                for (const rt of t4.keys()) if (!overall.has(rt)) overall.set(rt, 0);
                uncertain ||= !known2(host) || host.reasons.size > 0;
                if (!known2(host)) continue;
                anyKnown = true;
                for (const [rt, n3] of t4) overall.set(rt, overall.get(rt) + n3);
              }
              const chip = (host) => {
                const shown = value(sum(totals(host)), known2(host), host.reasons.size > 0), label = text(host) + " \xB7 Total running agents: " + shown, w5 = warn(host);
                return h5(
                  "span",
                  { class: "host-chip" + (!known2(host) ? " unknown" : ""), "data-host": host.name, role: "img", "aria-label": label, title: label, key: host.name },
                  h5("strong", null, short.get(host.name)),
                  h5("span", { class: "host-number" }, shown),
                  !known2(host) && host.age_text && h5("span", { class: "meta" }, host.age_text),
                  w5.length > 0 && h5("span", { class: "host-warning", title: "Sign-in: " + w5.join(" \xB7 "), "aria-label": "Sign-in: " + w5.join(" \xB7 ") }, icon("circle-alert"))
                );
              };
              const completeness = anyKnown && [...overall.values()].some((n3) => n3 > 0) ? "Partial" : "Unknown";
              const explanation = "All observed hosts \xB7 running agents" + (uncertain ? " \xB7 " + completeness + ": confirmed counts are lower bounds; additional agents unknown. " + [...sorted.filter((host) => !known2(host) || host.reasons.size).map((host) => host.name + " \xB7 " + (host.age_text || "age unknown") + " \xB7 " + [...host.reasons].join("; ")), ...sources].join("; ") : " \xB7 complete fresh observations");
              if (compact) return { label: strip.map(text).join("; ") || "Host unknown", meta: strip.length + " hosts \xB7 " + (uncertain ? "observations incomplete" : "current observations"), strip: h5("span", { class: "host-overview" }, h5("span", { class: "host-strip" }, strip.length ? strip.map(chip) : h5("span", { class: "host-chip unknown" }, "Hosts ?")), h5("span", { class: "host-runtime-totals", title: explanation, "aria-label": explanation }, [...overall].map(([rt, n3]) => runtime(rt, n3, anyKnown, uncertain)), uncertain && h5("span", { class: "host-completeness" }, completeness))) };
              return h5(
                "section",
                { class: "group running-agents", style: { "--host-number-width": numberWidth }, "aria-label": "Running agents" },
                h5("div", { class: "host-grid-head", "aria-label": "Host, Codex, Claude, Antigravity, Total running agents" }, h5("span", null, "Host"), ["Codex", "Claude", "Antigravity", "Total"].map((rt) => h5("span", { title: rt, "aria-label": rt, role: "img" }, icon(mark(rt))))),
                out,
                h5("div", { class: "body meta" }, "Registered project boards \xB7 " + (partial || all.some((c4) => c4.n === null) ? "Tasks unknown" : "Tasks: " + all.filter((c4) => ["doing", "review"].includes(c4.state)).map((c4) => c4.name + " " + c4.n).join(" \xB7 "))),
                h5("details", { "data-view-key": "run-definitions" }, api.disclosure("info", "Count definitions"), h5("div", { class: "body meta" }, "Running processes, occupied slots, reservations and board tasks are counted apart. \u2265 means a confirmed minimum; Partial means additional agents are unknown; ? means no confirmed total. Reservations include pending or unconfirmed work; occupied slots can include other projects."))
              );
            },
            home: function() {
              const faults = [api.agentq(), setup(), ...view.S.projects.flatMap((p5) => [problem(p5), incident(p5, true), incidentNotice(p5)])].filter(Boolean);
              const line = api.runningAgents(true);
              return f4(api.staleLine(), h5("details", { class: "dashboard-status", "data-view-key": "dashboard-status" }, h5("summary", null, line.strip, faults.length > 0 && h5("span", { class: "attention", role: "img", "aria-label": "Attention \xB7 " + faults.length, title: "Attention \xB7 " + faults.length }, icon("circle-alert"), String(faults.length))), api.runningAgents(), controls(), faults), dashboard.list());
            },
            routeBody: function() {
              switch (view.page) {
                case "agentq-shown":
                case "agentq-setting-up":
                case "agentq-running":
                case "agentq-unknown":
                  return api.agentq(view.page === "agentq-running");
                case "projects":
                  return projects();
                case "attention":
                  return attention() || h5("div", { class: "body meta" }, known() ? "No reported questions or failures" : "Status incomplete; check Services");
                case "settings":
                  return api.settings();
                case "hosts":
                  return f4(api.heading("Hosts and resources", "network"), api.runningAgents());
                case "delivery":
                  return f4(h5("p", { class: "meta" }, "Delivery is project policy, separate from Access. Exact-candidate approval and publication remain on the board."), view.S.projects.map((p5) => h5("details", { class: "group", "data-view-key": "delivery:" + p5.root }, api.disclosure("git-pull-request", [p5.name + " \xB7 ", h5("span", { class: "meta" }, p5.delivery?.preset || "policy unavailable")]), h5("div", { class: "body" }, { "auto-merge": "After review and required checks, publish and merge automatically.", "review-pr": "Open a pull request; the owner must approve its exact candidate before publication.", "direct": "Publish to main after review and required checks, using the board\u2019s direct route (a fast-forward MR on GitLab)." }[p5.delivery?.preset] || "Delivery policy unavailable; no publication authority is inferred."), h5("details", { "data-view-key": "delivery-policy:" + p5.root }, api.disclosure("info", "Policy details"), h5("div", { class: "body meta" }, "Revision " + (p5.delivery?.revision ?? "unknown"))), p5.issues && h5("a", { class: "project", href: p5.issues, "data-url": p5.issues }, "Open Board"))));
                case "appearance":
                  return appearance();
                case "cadence":
                  return history2();
                case "notifications":
                  return h5("button", { "data-a": "workspace", "data-page": "conversations", "data-filter": "new" }, "Open New in Conversations");
                case "notification-settings":
                  return notifications();
                case "agents":
                  return agents();
                case "work":
                  return f4(h5("div", { class: "body", role: "status" }, api.admissionText(view.S.projects, true)), api.balancingSections());
                case "stop":
                  return api.stopPanel();
                case "recovery":
                  return f4(api.heading("Hold on " + (view.S.host || "unknown host"), "rotate-ccw"), h5("div", { class: "body", role: "status" }, view.S.stop_hold ? "Admission held \xB7 " + (view.S.stop_hold.mode || "mode unknown") + (view.S.stop_hold.selected_count != null ? " \xB7 " + view.S.stop_hold.selected_count + " selected" : "") : "No emergency hold reported"), api.hostService(), h5("div", { class: "body meta" }, "Recovery is for selected local tasks. Ordinary Resume cannot clear an emergency hold; unknown effects stay held."), api.recoveryPanel());
                case "services":
                  return f4(api.heading(view.S.host || "This host", "activity"), api.hostService(), view.S.projects.map((p5) => problem(p5)), api.operationLauncher("service", null, "Host maintenance in workspace\u2026"), h5("details", { "data-view-key": "service-projects" }, api.disclosure("folder", "Project observations \xB7 " + view.S.projects.length), view.S.projects.map((p5) => [incidentNotice(p5), service(p5, false)])), view.S.agentq && api.heading("AgentQ", "bot"), view.S.agentq && api.agentq(true));
                case "about":
                  return about();
                default:
                  return home();
              }
            },
            resourceGroup: function(p5) {
              return { ...p5.resources, key: p5.root, labels: { host: p5.cap_host || "this host", project: p5.name } };
            },
            resourceCapability: function(g5) {
              const p5 = view.S && view.S.projects.find((p6) => p6.root === g5.key), e4 = p5 && p5.cap_saved && p5.cap_saved.error;
              return e4 || !p5 ? { edit: false, reason: "Read-only here", note: e4 ? "Config: " + e4 : null } : p5.resources?.targets?.some((t4) => t4.supported?.some((s4) => s4.policy_schema === "taskq.host-capacity/1")) ? { edit: require_resource_controls().fields(g5).length > 0, reason: "Editing unavailable here", note: "Host limits affect all registered projects. Project ceilings are read-only here." } : { edit: false, reason: "Legacy editor unavailable; refresh host status" };
            },
            resourceReceipt: function(m5) {
              const key = m5.root + "#" + m5.id, r4 = resourceRequests.get(key);
              if (!r4) return;
              cancel(r4.timer);
              resourceRequests.delete(key);
              const receipt = ["saved", "rejected", "unknown"].includes(m5.state) && (m5.state !== "saved" || typeof m5.saved_revision === "string" && /^[a-f0-9]{32}$/.test(m5.saved_revision)) ? { state: m5.state, reason: m5.reason, saved_revision: m5.saved_revision } : { state: "unknown", reason: "Invalid capacity receipt" };
              if (receipt.state === "rejected" && /host policy changed|revision/i.test(receipt.reason || "")) send({ a: "refresh" });
              if (menu?.querySelectorAll) mountResources();
              r4.resolve(receipt);
              defer(fit, 0);
            },
            capacity: function(p5) {
              if (!ports.workspace) return p5.resources ? api.operationLauncher("resources", p5.root, "Review capacity\u2026") : "";
              if (!p5.resources) return "";
              return h5("details", { class: "group", "data-view-key": "capacity:" + p5.root }, h5("summary", null, icon("sliders-horizontal"), "Capacity ", h5("span", { class: "meta" }, (p5.cap_host || "this host") + " \xB7 " + require_resource_controls().headline(resourceGroup(p5), view.S.now))), h5("div", { "data-resources": p5.root }));
            },
            workEntry: function(root) {
              return view.S.projects.some((p5) => p5.resources) ? h5("button", { class: "project", "data-a": "work", "data-root": root || null }, icon("focus"), "Balancing\u2026") : "";
            },
            balancingSections: function(hostControls, projectControls) {
              const availability = (p5) => p5.capacity?.reasons?.filter((r4) => ["usage-limit", "login", "capability", "access"].includes(r4.code)).map((r4) => r4.text || r4.code).join(" \xB7 ") || (p5.alive === true && Array.isArray(p5.capacity?.reasons) ? "" : "Availability unknown");
              const exceptions = view.S.projects.filter((p5) => availability(p5));
              return f4(
                api.heading("Task routing", "focus"),
                h5("div", { class: "body meta" }, "run-* pins stay strict. prefer-* and project routing are soft preferences; eligible subscription CLIs provide fallback."),
                h5("details", { "data-view-key": "balancing-routing" }, api.disclosure("focus", "Project routing \xB7 " + view.S.projects.length + " projects"), view.S.projects.map((p5) => h5("div", { class: "body meta" }, p5.name + " \xB7 " + (Array.isArray(p5.routing) ? p5.routing.length ? p5.routing.map((r4) => r4.labels.join(" + ") + " \u2192 " + r4.prefer).join("; ") : "Host default" : "Routing unknown")))),
                api.heading("Runtime availability", "bot"),
                exceptions.length ? exceptions.map((p5) => h5("div", { class: "body meta" }, p5.name + " \xB7 " + availability(p5))) : h5("div", { class: "body meta" }, "No reported availability blockers \xB7 " + view.S.projects.length + " projects"),
                h5("details", { "data-view-key": "balancing-access" }, api.disclosure("info", "Runtime access details"), api.accessFacts()),
                api.heading("Host balancing", "focus"),
                h5("div", { class: "body meta" }, "Host " + (view.S.host || "unknown") + " \xB7 limits and load gate affect all registered projects. Quota holds never change configured limits. Current tasks continue."),
                hostControls || (view.S.projects.some((p5) => p5.resources) ? api.operationLauncher("resources", null, "Review host limits\u2026") : h5("div", { class: "body meta" }, "Capacity unavailable: no supported host projection.")),
                api.heading("Project policies", "folder"),
                projectControls,
                api.operationLauncher("admission", null, "Review project admission\u2026"),
                h5("details", { "data-view-key": "settings-admission" }, api.disclosure("focus", "Project admission details"), h5("div", { class: "body meta" }, api.admissionText()), view.S.projects.map((p5) => h5("div", { class: "body meta" }, p5.name + " \xB7 " + (modeText(p5) || "Intake unknown") + " \xB7 " + (p5.capacity?.reasons?.map((r4) => r4.text || r4.code).join(" \xB7 ") || (p5.alive === true && Array.isArray(p5.capacity?.reasons) ? "No reported blockers" : "Admission observation unknown")))))
              );
            },
            workPage: function(root) {
              const ps = view.S.projects.filter((p6) => p6.resources), p5 = ps.find((p6) => p6.root === root || view.working?.[p6.root]) || ps[0], run = view.S.projects.filter((p6) => mode(p6) === "run"), held = view.S.projects.filter((p6) => mode(p6) === "paused");
              const editor = ports.workspace ? f4(api.resourceMeters(p5), p5?.resources.targets?.some((t4) => t4.supported?.some((s4) => s4.policy_schema === "taskq.host-capacity/1")) ? h5("div", { "data-resources": p5.root }) : h5("div", { "data-work": "" })) : api.operationLauncher("resources", Object.keys(view.working || {})[0] || null, "Review host limits in workspace\u2026");
              return f4(h5("div", { class: "body", role: "status" }, "Admission \xB7 " + api.admissionText()), api.balancingSections(editor, bulk(run, held)));
            },
            admissionText: function(ps = view.S.projects, compact = false) {
              if (!ps.length) return "No projects";
              const rows = ps.map((p5) => {
                const c4 = p5.capacity, reasons = Array.isArray(c4?.reasons) ? c4.reasons : null;
                const unknown = p5.alive !== true || !reasons || !["open", "off", "held"].includes(c4?.gate) || c4.gate === "held" && !reasons.length;
                return { name: p5.name, state: unknown ? "unknown" : reasons.length ? "blocked" : "clear", text: unknown ? "observation unknown" : reasons.length ? [...new Set(reasons.map((r4) => r4.code || "unknown"))].join(", ") : "no reported blockers" };
              });
              if (compact) return ps.length + " projects \xB7 " + (rows.every((r4) => r4.state === "clear") ? "no reported blockers" : [["blocked", "blocked"], ["unknown", "unknown"]].map(([state, label]) => [rows.filter((r4) => r4.state === state).length, label]).filter(([n3]) => n3).map(([n3, label]) => n3 + " " + label).join(" \xB7 "));
              const states = new Set(rows.map((r4) => r4.state)), lead = states.has("unknown") ? "Unknown" : states.size > 1 ? "Mixed" : states.has("blocked") ? "Blocked work" : "No reported blockers";
              const intake = [["run", "configured"], ["paused", "paused"], ["watch", "observation"]].map(([m5, l5]) => [l5, ps.filter((p5) => mode(p5) === m5).length]).filter(([, n3]) => n3).map(([l5, n3]) => n3 + " " + l5);
              const missing = ps.filter((p5) => !["run", "paused", "watch"].includes(mode(p5))).length;
              if (missing) intake.push(missing + " unknown");
              const same = rows.every((r4) => r4.text === rows[0].text);
              const scope = same && rows.length > 1 ? "all " + rows.length + " projects" + (rows[0].state === "blocked" ? " (" + rows[0].text + ")" : "") : rows.map((r4) => r4.name + " (" + r4.text + ")").join(" \xB7 ");
              return lead + ": " + scope + " \xB7 Intake: " + intake.join(" \xB7 ");
            },
            resourceMeters: function(p5) {
              const c4 = p5?.capacity, known2 = p5?.alive === true && !!p5.sample_age_text && !c4?.reasons?.some((r4) => r4.code === "sample");
              return h5("div", { class: "resource-meters" }, [["CPU busy", c4?.cpu_percent], ["Memory available", c4?.memory_available_percent]].map(([label, value]) => {
                const valid = known2 && Number.isFinite(value) && value >= 0 && value <= 100;
                return h5("div", { class: "resource-meter" }, h5("span", null, label), valid && h5("meter", { min: 0, max: 100, value, "aria-label": label }), h5("span", null, valid ? Math.round(value) + "%" : "Unknown"));
              }), h5("small", { class: "meta" }, known2 ? "Sample " + p5.sample_age_text + " ago" : "No current sample"));
            },
            mountResources: function() {
              const ws = menu.querySelector("[data-work]");
              if (ws) {
                if (!view.workCard) {
                  const el = document2.createElement("div");
                  view.workCard = { el, ui: require_resource_controls().mountWork(el, { capability: resourceCapability, send: resourceSend, now: () => view.S.now, host: view.S.host }, view.working) };
                }
                if (ws.firstElementChild !== view.workCard.el) ws.replaceChildren(view.workCard.el);
                view.workCard.ui.update(view.S.projects.filter((p5) => p5.resources).map(resourceGroup));
              }
              for (const slot of menu.querySelectorAll("[data-resources]")) {
                const p5 = view.S.projects.find((p6) => p6.root === slot.dataset.resources);
                if (!p5) continue;
                const scope = p5.resources.targets?.[0]?.supported?.[0]?.policy_schema;
                let card = resourceCards.get(p5.root);
                if (!card || card.scope !== scope) {
                  card?.el.remove();
                  const el = document2.createElement("div");
                  card = { el, scope, ui: require_resource_controls().mount(el, { capability: resourceCapability, send: resourceSend, now: () => view.S.now }) };
                  resourceCards.set(p5.root, card);
                }
                if (card.el.parentNode !== slot) slot.append(card.el);
                card.ui.update(resourceGroup(p5));
              }
            },
            notifications: function() {
              const N2 = view.S.notify, st = N2.status, line = { allowed: "Allowed by the OS", blocked: "Blocked by the OS: nothing is posted", "not asked": "Not asked yet: the first banner setting asks the OS", asking: "Asked: answer the OS prompt", "request failed": "OS permission request failed: " + N2.failure, unknown: "OS permission unknown", unsupported: "Not available on this host" }[st] || "OS permission unknown";
              return f4(
                api.heading("OS permission", "shield"),
                h5("div", { class: "body" + (st === "blocked" || st === "request failed" ? " alert" : " meta"), role: "status" }, line),
                st === "blocked" && h5("button", { class: "project", "data-a": "notify-os" }, "Open OS setting \u2197"),
                api.heading("Per event on this host", "bell"),
                h5("p", { class: "meta notification-legend" }, "Off: no OS delivery \xB7 Silent: no OS banner \xB7 Banner: OS banner \xB7 Sound: banner with sound. History is independent; banners and sound need OS permission."),
                view.S.display.notify.map((x5) => api.notifyRow(x5, null, N2.host[x5.id], st)),
                api.heading("Project overrides", "folder"),
                view.S.projects.map((p5) => api.notifyGroup(p5))
              );
            },
            notifyRow: function(event, root, value, status) {
              const choices = [...root ? [["inherit", "Inherit"]] : [], ...modes.map(([v4, label]) => [v4, v4 === "sound" ? "Sound" : label])];
              return h5("div", { class: "notification-row" }, h5("label", null, h5("span", null, event.name), h5(
                "select",
                { "data-notify": event.id, "data-root": root || null, value: value || "unknown", "aria-label": event.name + (root ? " for " + (view.S.projects.find((p5) => p5.root === root)?.name || "project") : " on this host") },
                !choices.some(([v4]) => v4 === value) && h5("option", { value: "unknown", disabled: true }, "Unknown"),
                choices.map(([v4, label]) => h5("option", { value: v4, selected: v4 === value }, label))
              )), !root && h5("button", { "data-a": "notify-test", "data-kind": event.id, disabled: status !== "allowed", title: "Send " + event.name + " test notification" }, "Test"));
            },
            notifyGroup: function(p5) {
              const N2 = view.S.notify?.projects?.[p5.root];
              if (!N2) return h5("div", { class: "body meta" }, p5.name + ": notification overrides unknown; no project readback");
              const name = (v4) => modes.find((m5) => m5[0] === v4)?.[1] || "Unknown";
              return h5(
                "details",
                { class: "group", "data-view-key": "notifications:" + p5.root },
                api.disclosure("bell", [p5.name, " \xB7 ", h5("span", { class: "meta" }, view.S.display.notify.some((x5) => !N2[x5.id]) ? "Overrides unknown" : view.S.display.notify.every((x5) => !N2[x5.id][0]) ? "Inherit host" : view.S.display.notify.filter((x5) => N2[x5.id][0]).length + " override" + (view.S.display.notify.filter((x5) => N2[x5.id][0]).length === 1 ? "" : "s"))]),
                view.S.display.notify.map((x5) => f4(api.notifyRow(x5, p5.root, N2[x5.id] ? N2[x5.id][0] || "inherit" : "unknown", view.S.notify.status), h5("div", { class: "meta" }, "Effective: " + name(N2[x5.id]?.[1]))))
              );
            },
            openCard: function(o4) {
              const p5 = view.S?.projects.find((p6) => p6.root === o4.root);
              if (p5) send({ a: "workspace", open: { page: "conversations", root: p5.root, board: p5.issues, task: o4.n, ...o4.sha ? { sha: o4.sha } : {}, ...o4.id ? { event: o4.id } : {} } });
            },
            project: function(p5) {
              const ts = tasks(p5), counters = TaskqProjectBlocks.counters(p5.counts, p5.root, p5.name);
              const groups = view.S.display.states.map(({ id: st, name }) => {
                const r4 = ts.filter((t4) => t4.state === st);
                return r4.length ? h5("div", { class: "group", key: st }, h5("h2", { class: "section-heading task-group-heading" }, h5("span", null, name), counter(r4.length, color(st), name), r4.length > 1 && startButton(r4)), r4.map((t4) => taskRow(t4))) : "";
              }).filter(Boolean);
              return f4(
                h5("section", { class: "tqp", dangerouslySetInnerHTML: { __html: counters } }),
                h5("div", { class: "body meta" }, modeText(p5)),
                problem(p5),
                incident(p5, true),
                incidentNotice(p5),
                groups.length ? groups : h5("div", { class: "body muted" }, "No open tasks"),
                answerNotices(p5.issues),
                h5("div", { class: "group" }, admission(p5)),
                capacity(p5),
                notifyGroup(p5),
                workEntry(p5.root),
                service(p5)
              );
            },
            filteredTasks: function(p5) {
              if (p5 && !view.dashboard) {
                view.dashboard = require_task_views().normalize(null, { taskFilter: view.taskFilter });
                require_task_views().current(view.dashboard).working.projects = [require_task_views().identity(p5)];
              }
              return dashboard.list();
            },
            heading: function(label, mark) {
              return h5("h2", { class: "section-heading" }, icon(mark), label);
            },
            disclosure: function(mark, children) {
              return h5("summary", { class: "settings-disclosure" }, icon(mark), h5("span", { class: "disclosure-label" }, children));
            },
            stickyFilters: function() {
              const content = menu.querySelector(".content"), row = content?.querySelector(".filter-header"), host = content?.querySelector(".dashboard-status>summary"), height = host?.getBoundingClientRect().height || 0;
              if (content) {
                content.style.setProperty("--host-strip-height", height + "px");
                content.style.scrollPaddingTop = row || host ? height + (row?.getBoundingClientRect().height || 0) + "px" : "";
              }
              if (row) row.classList.toggle("pinned", content.scrollTop > 0 && row.getBoundingClientRect().top <= content.getBoundingClientRect().top + height + 1);
            },
            settings: function() {
              const S3 = view.S, ps = S3.projects, A4 = S3.agents, st = S3.notify?.status, svc = S3.service?.state, up = ps.find((p5) => p5.update && p5.update !== p5.version);
              const growth = { auto: "Auto", top: "Down", bottom: "Up", fixed: "Fixed" }[S3.size_anchor || "auto"] || "Auto";
              const widget = (target, mark, label, meta) => h5("button", { class: "project settings-widget", ...target }, mark, h5("span", null, label), h5("span", { class: "meta" }, meta));
              const sections = [
                ["Display", [
                  widget({ "data-route": "appearance" }, icon("type"), "Appearance", S3.font + " px \xB7 " + (S3.view === "full" ? "Summary" : "Icon") + " \xB7 growth " + growth),
                  widget({ "data-route": "notification-settings" }, icon("bell"), "Notifications", { allowed: "OS allowed", blocked: "OS blocked", "not asked": "OS not asked", asking: "OS asking", "request failed": "OS request failed", unsupported: "OS unsupported" }[st] || "OS permission unknown"),
                  widget({ "data-route": "cadence" }, icon("chart-no-axes-column"), "Activity history", S3.display?.periods?.find((x5) => x5.id === S3.period)?.name || "period unknown")
                ]],
                ["Host and projects", [
                  widget({ "data-route": "hosts" }, icon("network"), "Hosts and resources", api.runningAgents(true).meta),
                  widget({ "data-route": "work" }, icon("focus"), "Balancing", api.admissionText(ps, true) + (ps.some((p5) => p5.resources) ? "" : " \xB7 capacity unavailable")),
                  widget({ "data-route": "agents" }, icon("bot"), "Agents and access", A4?.checking ? "checking sign-in" : A4 ? A4.rows.map((r4) => runtimeName(r4.runtime) + " " + (r4.state === "signed-in" && !A4.fresh ? "signed in at capture" : (signState[r4.state] || "Unknown").toLowerCase())).join(" \xB7 ") || "no runtime" : "sign-in unknown"),
                  widget({ "data-route": "projects" }, icon("folder"), "Projects", ps.length + " registered"),
                  widget({ "data-route": "delivery" }, icon("git-pull-request"), "Delivery", ps.length + " projects \xB7 " + new Set(ps.map((p5) => p5.delivery?.preset || "unavailable")).size + " " + (new Set(ps.map((p5) => p5.delivery?.preset || "unavailable")).size === 1 ? "policy" : "policies")),
                  widget({ "data-route": "services" }, icon("activity"), "Services and support", "host service " + (svc === "running" ? S3.service.fresh ? "running" : "stale observation" : svc === "exited" ? "exited" : "unknown")),
                  S3.stop_modes && widget({ "data-a": "workspace", "data-page": "operations", "data-operation": "stop" }, icon("octagon-x"), "Emergency stop", "Drain or Full stop"),
                  S3.stop_modes && widget({ "data-route": "recovery" }, icon("rotate-ccw"), "Recovery", S3.stop_hold ? "admission held" : "no hold reported"),
                  widget({ "data-route": "about" }, icon("info"), "About and updates", (S3.version ? "v" + S3.version : "version unknown") + (up ? " \xB7 update " + up.update : ""))
                ]]
              ];
              return f4(sections.map(([label, rows]) => h5("section", { class: "settings-section" }, api.heading(label, label === "Display" ? "type" : "network"), rows)));
            },
            windowSettings: function() {
              const mode2 = view.S.size_anchor || "auto";
              return f4(h5("p", { class: "meta" }, "Auto follows the actual tray opening direction. Dragging pins the window; explicit growth choices stay selected."), h5("fieldset", null, h5("legend", null, "Window growth"), h5("div", { class: "growth-options" }, [["auto", "Auto"], ["top", "Down"], ["bottom", "Up"], ["fixed", "Fixed"]].map(([value, label]) => h5("button", { "data-a": "size-anchor", "data-value": value, "aria-pressed": String(mode2 === value) }, h5("span", { class: "growth-miniature", "data-growth": value, "data-opening": view.S.auto_anchor === "bottom" ? "up" : "down", "aria-hidden": "true" }, h5("i")), h5("span", null, label))))), h5("div", { class: "options" }, button(view.S.pinned ? "pin-off" : "pin", "Keep menu open", { "data-a": "pin", "aria-pressed": String(!!view.S.pinned) }), button("always-on-top", "Menu always on top", { "data-a": "top", "aria-pressed": String(!!view.S.top) })));
            },
            head: function() {
              const p5 = view.S.projects?.find((p6) => p6.root === view.page), title = view.working ? "Balancing" : p5 ? p5.name : routes[view.page] || "Unavailable";
              const crumbs = p5 ? h5("a", { href: "#", "data-route": "projects" }, "Projects") : settingsPages.includes(view.page) && view.page !== "settings" ? h5("a", { href: "#", "data-route": "settings" }, "Settings") : "";
              if (ports.settings) {
                const W = view.settingsChrome || {};
                return shell.bar({ title, crumbs, home: "Settings home", back: view.page !== "settings" || renderer.entries().length > 0, actions: [], windows: [button("always-on-top", "Always on top", { "data-a": "settings-top", "aria-pressed": String(!!W.top) }), button(W.maximized ? "minimize-2" : "maximize-2", W.maximized ? "Restore window" : "Enlarge window", { "data-a": "settings-enlarge", "aria-pressed": String(!!W.maximized) }), button("x", "Close settings", { "data-a": "hide" })] });
              }
              if (ports.views) return shell.bar({ title: "View \xB7 " + dashboard.current().name, actions: [], windows: [button("x", "Close view window", { "data-a": "hide" })] });
              const actions = [p5?.issues && h5("a", { href: p5.issues, "data-url": p5.issues, title: "Open Board", "aria-label": "Open " + p5.name + " Board" }, icon(p5.board_kind)), view.S.stop_modes && button("octagon-x", "Emergency stop", { "data-a": "stop-review" }), notificationPanel.entry(), button("settings", "Settings", { "data-route": "settings" })];
              const windows = [button(view.S.pinned ? "pin-off" : "pin", "Keep menu open", { "data-a": "pin", "aria-pressed": String(!!view.S.pinned) }), button("always-on-top", "Always on top", { "data-a": "top", "aria-pressed": String(!!view.S.top) }), button("x", "Close menu", { "data-a": "hide" })];
              return shell.bar({ title: view.page === "home" ? "Tasks" : title, crumbs, home: "Home", back: view.page !== "home" || renderer.entries().length > 0 || !!view.working, actions, windows });
            },
            draw: function(navigating = false) {
              if (!view.S || view.resizing || ports.views && !view.dashboard) return;
              const firstRender = !view.drawn;
              const active = document2.activeElement, scroll = (menu.querySelector(".content") || document2.scrollingElement).scrollTop;
              const bounds = new Map([...menu.querySelectorAll("[data-bound]")].map((e4) => [e4.dataset.bound, e4.scrollTop]));
              const focusKey = !navigating && active && menu.contains(active) ? JSON.stringify(active.dataset) : null;
              const same = (e4) => e4.tagName === active.tagName && JSON.stringify(e4.dataset) === focusKey;
              const focusAt = focusKey ? [...menu.querySelectorAll(active.tagName)].filter(same).indexOf(active) : -1;
              const focusedSummary = !navigating && active?.tagName === "SUMMARY" ? [...menu.querySelectorAll("summary")].indexOf(active) : -1;
              const open = navigating || view.restoreScroll != null ? view.disclosures : viewState.groups(), p5 = view.S.projects.find((p6) => p6.root === view.page);
              if (!p5 && !routes[view.page]) view.page = "home";
              if (ports.settings && view.page === "home") view.page = "settings";
              document2.documentElement.style.colorScheme = view.S.theme || "light dark";
              menu.style.setProperty("--size", view.S.font + "px");
              const smart = !ports.workspace && !ports.settings && !ports.views && view.S.opacity_mode === "background";
              const translucent = !ports.workspace && !ports.settings && !ports.views && !smart && (view.S.opacity ?? 100) < 100;
              for (const entry of ((translucent ? view.S.opacity_palette : view.S.palette) || "").split(";")) {
                const [key, value] = entry.split(":");
                if (key && value) menu.style.setProperty(key, value);
              }
              menu.classList.toggle("translucent", translucent);
              menu.classList.toggle("background-only", smart);
              menu.style.setProperty("--background-alpha", (view.S.background_opacity ?? 100) + "%");
              menu.classList.toggle("pinned", !!view.S.pinned);
              const edge = view.S.attached_edge || "top";
              if (menu.dataset.edge !== edge) menu.dataset.edge = edge;
              const settingsError = view.persistenceError || view.S.opacity_error || view.S.settings_error;
              const title = view.working ? "Balancing" : view.taskFilter ? "Tasks" : p5 ? p5.name : routes[view.page];
              const tree = ports.views ? [head(), h5("div", { class: "content" }, h5("div", { class: "content-body view-window" }, dashboard.presets(), dashboard.conditions())), h5("footer", { class: "view-window view-window-footer" }, dashboard.actions())] : [head(), h5("div", { class: "content" }, h5("div", { class: "content-body" + (settingsPages.includes(view.page) || view.page === "projects" ? " settings-content" : "") }, view.unavailable && h5("div", { class: "body", role: "status" }, view.unavailable + " The surviving destination is " + title + "."), actionNotices(), settingsError && h5("div", { class: "body alert", role: "alert" }, "Settings unknown: " + settingsError + (view.persistenceError || view.S.opacity_error ? "" : "; defaults shown")), view.working ? workPage() : view.taskFilter ? filteredTasks(p5) : p5 ? project(p5) : routeBody())), h5("footer", { class: "status-footer" }, h5("span", null, "TaskQ ", view.S.version ? "v" + view.S.version : "version unknown"), h5("div", { class: "grip", "data-resize": "", role: "button", "aria-label": "Resize window with arrow keys", title: "Drag to resize \xB7 arrow keys", tabindex: "0" })), dashboard.popup(), answerToast()];
              renderer.render(tree, p5 ? "/projects/" + encodeURIComponent(p5.issues) + (view.taskFilter ? "/tasks" : "") : view.working ? "/work" : "/" + view.page);
              view.drawn = true;
              if (navigating) menu.querySelector(".shell-overflow")?.removeAttribute("open");
              mountResources();
              menu.querySelectorAll("details[data-view-key]:not(.shell-overflow)").forEach((d5) => d5.open = !d5.classList.contains("answered") && (view.page === "attention" || view.page === "cadence" && d5.classList.contains("hist")));
              viewState.restore(open);
              if (view.S.page !== void 0 && (navigating || firstRender)) menu.querySelectorAll(".hist,.agents,.service,.incident-notice").forEach((d5) => d5.open = !(p5 && d5.classList.contains("service")));
              if (!navigating) {
                const target = focusedSummary >= 0 ? menu.querySelectorAll("summary")[focusedSummary] : focusKey && active.tagName === "H1" ? menu.querySelector("h1") : focusKey && focusKey !== "{}" ? [...menu.querySelectorAll(active.tagName)].filter(same)[focusAt] : null;
                target?.focus({ preventScroll: true });
              }
              (menu.querySelector(".content") || document2.scrollingElement).scrollTop = view.restoreScroll ?? scroll;
              view.restoreScroll = null;
              api.stickyFilters();
              if (firstRender) viewState.focus(view.restoreFocus);
              fit();
              dashboard.place();
              const bound = [...menu.querySelectorAll("[data-bound]")];
              bound.forEach((c4) => {
                c4.scrollTop = bounds.has(c4.dataset.bound) ? bounds.get(c4.dataset.bound) : 0;
                wholeLines(c4);
              });
              if (bound.some((c4) => c4.style.maxHeight)) fit();
              if (ports.views) {
                const text = JSON.stringify(view.dashboard);
                if (text !== view.sentDashboard) {
                  view.sentDashboard = text;
                  send({ a: "views-dashboard", dashboard: view.dashboard });
                }
              }
              viewState.flush();
            },
            wholeLines: function(c4) {
              if (c4.scrollHeight <= c4.clientHeight) return;
              const top = c4.getBoundingClientRect().top + c4.clientTop;
              const lines2 = () => {
                const r4 = document2.createRange(), w5 = document2.createTreeWalker(c4, NodeFilter.SHOW_TEXT), a4 = [];
                for (let n3; n3 = w5.nextNode(); ) {
                  r4.selectNodeContents(n3);
                  a4.push(...r4.getClientRects());
                }
                return a4;
              };
              const shrink = (d5) => c4.style.maxHeight = c4.offsetHeight - d5 + "px";
              const cut = (edge) => Math.min(...lines2().filter((l5) => l5.top < edge && l5.bottom > edge).map((l5) => l5.top));
              const head2 = cut(top);
              if (head2 < top && c4.scrollTop + c4.clientHeight < c4.scrollHeight - 1) c4.scrollTop -= top - head2;
              else if (head2 < top) {
                const d5 = Math.min(...lines2().filter((l5) => l5.top >= top).map((l5) => l5.top)) - top;
                if (d5 < c4.clientHeight) {
                  shrink(d5);
                  c4.scrollTop += d5;
                }
              }
              const bottom = top + c4.clientHeight, tail = cut(bottom);
              if (tail > top && tail < bottom) shrink(bottom - tail);
            },
            settle: function(c4) {
              if (!c4.isConnected) return;
              const s4 = c4.scrollTop;
              c4.style.maxHeight = "";
              c4.scrollTop = s4;
              wholeLines(c4);
              fit();
            },
            refresh: function(now) {
              if (view.pressedAt !== null && now - view.pressedAt < 5e3) {
                view.stale = true;
                return;
              }
              tick();
            },
            release: function() {
              view.pressedAt = null;
              if (view.stale) {
                view.stale = false;
                defer(present);
              }
            },
            batch: function(updates) {
              batching++;
              let complete = false;
              try {
                const snapshot = updates.filter((update2) => Array.isArray(update2?.projects)).pop();
                if (snapshot) receive(snapshot);
                for (const update2 of updates) if (!Array.isArray(update2?.projects)) receive(update2);
                complete = true;
              } finally {
                if (!--batching && staged) {
                  const args = staged;
                  staged = null;
                  if (complete) {
                    if (view.pressedAt !== null && performance2.now() - view.pressedAt < 5e3) view.stale = true;
                    else present(...args);
                  }
                }
              }
            },
            receive: function(s4) {
              if (Array.isArray(s4.updates)) {
                api.batch(s4.updates);
                return;
              }
              if (s4.notificationReceipt) {
                notificationPanel.receive(s4.notificationReceipt);
                return;
              }
              if (s4.reportReceipt) {
                reports.receive(s4.reportReceipt);
                return;
              }
              if (Object.hasOwn(s4, "settingsWindow")) {
                if (ports.settings) view.settingsChrome = s4.settingsWindow;
                else {
                  view.settingsWindow = s4.settingsWindow === true;
                  view.viewsWindow = s4.viewsWindow === true;
                }
                if (view.S) present();
                return;
              }
              if (s4.dashboard) {
                if (!ports.views) dashboard.adopt(s4.dashboard);
                else if (s4.initial || !document2.hasFocus()) {
                  view.dashboard = require_task_views().normalize(s4.dashboard);
                  view.sentDashboard = JSON.stringify(view.dashboard);
                  present();
                }
                return;
              }
              if (s4.settingsOpen) {
                if (ports.settings && view.S && s4.settingsOpen.page !== view.page) navigate2(s4.settingsOpen.page);
                return;
              }
              if (Object.hasOwn(s4, "agentqError")) {
                view.agentqError = s4.agentqError;
                present();
                return;
              }
              if (ports.workspace && s4.projects) {
                api.adopt(s4);
                focusReadback();
                api.lifecycleReadback();
                present();
                return;
              }
              if (s4.answerReceipt) {
                startOperations.answerReceipt(s4.answerReceipt);
                return;
              }
              if (s4.replyFileReceipt) {
                startOperations.answerFileReceipt(s4.replyFileReceipt);
                return;
              }
              if (s4.workspaceError !== void 0) {
                view.workspaceError = s4.workspaceError;
                present();
                return;
              }
              if (s4.viewStateError !== void 0) {
                view.persistenceError = s4.viewStateError;
                present();
                return;
              }
              if (s4.diagnose) {
                const d5 = s4.diagnose, remaining = d5.until_ms - Date.now();
                if (d5.id !== view.diagnosticId && remaining > 0) {
                  execution.diagnose(d5.selector, Math.min(6e4, remaining), d5.limit);
                  view.diagnosticId = d5.id;
                }
                return;
              }
              if (s4.aiError) {
                view.aiSending = false;
                view.aiError = s4.aiError;
                present();
                return;
              }
              if (s4.completed) {
                dashboard?.receive(s4.completed);
                return;
              }
              if (!ports.workspace && ["action_error", "focus_receipt", "resources_receipt", "capmsg", "aiError"].some((k5) => s4[k5])) return;
              if (s4.action_error) {
                actionReceipt(s4.action_error);
                return;
              }
              if (s4.focus_receipt) {
                focusReceipt(s4.focus_receipt);
                return;
              }
              if (s4.resources_receipt) {
                view.resourceNotice = s4.resources_receipt;
                present();
                return;
              }
              if (s4.capmsg) {
                if (api.recoveryReceipt(s4.capmsg)) return;
                if (view.hostCommand?.id === s4.capmsg.id) api.lifecycleReceipt(s4.capmsg);
                else resourceReceipt(s4.capmsg);
                return;
              }
              if (s4.open) {
                openCard(s4.open);
                return;
              }
              if (view.S?.size_anchor !== s4.size_anchor) view.manualSize = null;
              if (!ports.settings && !ports.views) viewState.hydrate(s4);
              reports.sync(s4.projects || []);
              api.adopt(s4);
              view.receivedAt = performance2.now();
              focusReadback();
              api.lifecycleReadback();
              if (s4.page !== void 0 && s4.page !== view.page) {
                view.popup = null;
                view.page = s4.page;
                view.taskFilter = null;
                view.working = null;
                view.workCard = null;
                renderer.clear();
                view.disclosures = {};
                present(true);
                return;
              }
              if (document2.activeElement?.dataset.rscField) {
                mountResources();
                return;
              }
              if (document2.activeElement && (document2.activeElement.dataset.draft || document2.activeElement.dataset.recoveryEvidence || document2.activeElement.id === "ai")) return;
              refresh(performance2.now());
            },
            measure: function(name) {
              const e4 = document2.documentElement, m5 = menu, x5 = Math.max(e4.scrollWidth - e4.clientWidth, m5.scrollWidth - m5.clientWidth);
              if (x5 > 0) send({ a: "overflow", shot: name, x: x5 });
              const seen = /* @__PURE__ */ new Set();
              for (const b5 of m5.querySelectorAll("button")) {
                const text = b5.textContent.trim().slice(0, 24);
                if (b5.offsetParent && b5.scrollWidth > b5.clientWidth + 1 && !seen.has(text)) {
                  seen.add(text);
                  send({ a: "overflow", shot: name + " button " + text, x: b5.scrollWidth - b5.clientWidth });
                }
              }
            },
            mount: function() {
              window2.taskq = { settle, receive, measure, model: { unique, mode } };
              dashboard.mount();
              ui.listen(menu, "toggle", () => viewState.schedule(), true);
              ui.listen(menu, "scroll", (e4) => {
                const c4 = e4.target;
                if (c4.classList?.contains("content")) {
                  viewState.schedule();
                  api.stickyFilters();
                }
                if (!c4.dataset?.bound) return;
                cancel(c4.settling);
                c4.settling = defer(() => settle(c4), 150);
              }, true);
              ui.listen(menu, "pointerdown", () => {
                view.pressedAt = performance2.now();
              });
              for (const event of ["pointerup", "pointercancel"]) ui.listen(window2, event, release, true);
              ui.listen(window2, "blur", release);
              ui.listen(window2, "resize", () => present());
              ui.listen(menu, "click", (e4) => {
                if (dashboard.click(e4)) return;
                const crumb = e4.target.closest("a[data-route]");
                if (crumb) {
                  e4.preventDefault();
                  if (crumb.dataset.route !== view.page || view.working || view.taskFilter) navigate2(crumb.dataset.route);
                  return;
                }
                const cardLink = e4.target.closest(".qac a");
                if (cardLink) {
                  e4.preventDefault();
                  send({ a: "open", url: cardLink.getAttribute("href") });
                  return;
                }
                if (e4.target.closest(".qac,.rsc")) return;
                if (TaskqProjectBlocks.handle(e4, { open: (url) => send({ a: "open", url }), project: (id) => navigate2(id), tasks: (id, state) => {
                  const c4 = dashboard.criteria();
                  if (view.page !== "home") {
                    c4.projects = id ? [require_task_views().identity(view.S.projects.find((p5) => p5.root === id))] : [];
                    c4.states = [state];
                    navigate2("home");
                  } else dashboard.set({ states: c4.states.includes(state) ? c4.states.filter((s4) => s4 !== state) : [...c4.states, state] });
                } })) return;
                const a4 = e4.target.closest("a[data-url]");
                if (a4) {
                  e4.preventDefault();
                  send({ a: "open", url: a4.dataset.url });
                  return;
                }
                const b5 = e4.target.closest("button");
                if (!b5 || b5.disabled) return;
                const d5 = b5.dataset;
                if (notificationPanel.click(b5)) return;
                if (reports.click(b5, view.S.projects)) return;
                if (d5.historyBin !== void 0) {
                  view.historyBin = d5.historyBin;
                  present();
                  return;
                }
                if (!ports.workspace && api.operationTarget(d5)) {
                  send({ a: "workspace", open: { page: "operations", operation: api.operationTarget(d5), root: d5.root || null, ...ports.settings ? { origin: { surface: "settings", page: view.page } } : {} } });
                  return;
                }
                if (d5.a === "filter-reset") {
                  view.taskFilter = [];
                  present();
                  return;
                }
                if (d5.a === "all-tasks") {
                  if (view.page === "home" && view.taskFilter) {
                    view.taskFilter = [];
                    present();
                  } else navigate2("home", []);
                  return;
                }
                if (d5.a === "hide") viewState.flush();
                if (d5.a === "home") {
                  const root = ports.settings ? "settings" : "home";
                  if (view.page !== root || view.taskFilter || view.working) navigate2(root);
                  return;
                }
                if (d5.a === "workspace") {
                  if (ports.workspace && api.operationAction(d5)) return;
                  viewState.flush();
                  send({ a: "workspace", open: { page: d5.page || "conversations", ...d5.thread !== void 0 ? { thread: d5.thread } : {}, ...d5.filter ? { filter: d5.filter } : {}, operation: d5.operation || null, ...ports.settings ? { origin: { surface: "settings", page: view.page } } : {}, root: d5.root || null, board: d5.board || null, task: Number(d5.task) || null, revision: Number(d5.revision) || null, ...d5.fresh === "true" ? { fresh: true } : {} } });
                  return;
                }
                if (d5.route) {
                  if (d5.route !== view.page || view.working || view.taskFilter) navigate2(d5.route);
                  return;
                }
                if (d5.a === "answer-dismiss") {
                  startOperations.dismiss(d5.key);
                  return;
                }
                if (d5.a === "toast-dismiss") {
                  view.toast = null;
                  present();
                  return;
                }
                if (d5.a === "control") {
                  const t4 = unique(view.S.projects.flatMap(tasks)).find((t5) => t5.key === d5.key);
                  if (t4 && (t4.controls || []).includes(d5.control)) ui.effect("task.control", () => answerSend(t4.key.slice(0, -String(t4.n).length - 1), TaskqTaskControls.op(t4, d5.control, "tray")), () => {
                  });
                  present();
                  return;
                }
                if (d5.a === "start") {
                  api.startHeld(JSON.parse(d5.targets));
                  present();
                  return;
                }
                if (d5.a === "answer-retry") {
                  startOperations.retry(d5.key);
                  present();
                  return;
                }
                if (d5.a === "sync-retry") {
                  send({ a: "sync-retry", root: d5.root, id: d5.id });
                  return;
                }
                if (d5.a === "notify" || d5.a === "notify-test") {
                  send({ a: d5.a, kind: d5.kind, value: d5.v, root: d5.root });
                  return;
                }
                if (d5.a === "work") {
                  if (ports.workspace && api.operationAction(d5)) return;
                  navigate2(view.page, null, d5.root ? { [d5.root]: true } : {});
                  return;
                }
                if (d5.a === "back") {
                  back();
                  return;
                }
                if (d5.set === "period" && d5.v === "custom") {
                  const H2 = view.S.history;
                  view.draft = { start: local(H2.start, H2.tz), end: local(H2.end, H2.tz), tz: H2.tz, error: "" };
                  present();
                  return;
                }
                if (d5.set) {
                  view.S[d5.set] = ["font", "opacity", "background_opacity"].includes(d5.set) ? Number(d5.v) : d5.v;
                  if (d5.set === "period") view.draft = null;
                  send({ a: "set", [d5.set]: view.S[d5.set] });
                  present();
                  return;
                }
                if (d5.a === "range") {
                  const t4 = (v4) => v4 ? Date.parse(view.draft.tz === "UTC" ? v4 + "Z" : v4) / 1e3 : NaN;
                  const a5 = t4(view.draft.start);
                  const z3 = t4(view.draft.end);
                  view.draft.error = !(a5 < z3) ? "Refused: start must be before end." : "";
                  if (!view.draft.error) {
                    view.S.period = "custom";
                    send({ a: "set", period: "custom", start: a5, end: z3, tz: view.draft.tz });
                    view.draft = null;
                  }
                  present();
                  return;
                }
                if (api.operationAction(d5)) return;
                if (d5.a) {
                  actionErrors.delete(d5.root);
                  send({ a: d5.a, root: d5.root, label: d5.label, value: d5.value });
                }
                present();
              });
              ui.listen(menu, "change", (e4) => {
                if (e4.target.matches("[data-notify]")) send({ a: "notify", kind: e4.target.dataset.notify, value: e4.target.value, root: e4.target.dataset.root });
              });
              ui.listen(menu, "input", (e4) => {
                if (e4.target.matches("[data-opacity]")) {
                  const key = e4.target.dataset.opacity;
                  view.S[key] = Number(e4.target.value);
                  send({ a: "set", [key]: view.S[key] });
                  present();
                  return;
                }
                if (e4.target.matches("[data-history-select]")) {
                  view.historyBin = e4.target.value;
                  present();
                  return;
                }
                api.operationInput(e4);
                const k5 = e4.target.dataset.draft;
                if (k5 && view.draft) view.draft[k5] = e4.target.value;
              });
              ui.listen(menu, "focusout", (e4) => {
                const f5 = (x5) => x5 && (x5.id === "ai" || x5.dataset && (x5.dataset.draft || x5.dataset.rscField || x5.dataset.recoveryEvidence));
                if (f5(e4.target)) defer(() => {
                  if (!f5(document2.activeElement)) present();
                });
              });
              ui.listen(menu, "pointerdown", (e4) => {
                if (view.S && e4.button === 0 && e4.target.closest(".head") && !e4.target.closest("button,a,summary")) view.headerDrag = { x: e4.screenX, y: e4.screenY };
              });
              ui.listen(window2, "pointermove", (e4) => {
                const start = view.headerDrag;
                if (start && Math.hypot(e4.screenX - start.x, e4.screenY - start.y) >= 4) {
                  view.headerDrag = null;
                  send({ a: "drag" });
                }
              });
              for (const event of ["pointerup", "pointercancel"]) ui.listen(window2, event, () => {
                view.headerDrag = null;
              });
              ui.listen(menu, "pointerdown", (e4) => {
                if (e4.button !== 0 || !e4.target.closest(".grip")) return;
                view.resizing = { x: e4.screenX, y: e4.screenY, w: window2.innerWidth, h: window2.innerHeight };
                e4.target.setPointerCapture(e4.pointerId);
                e4.preventDefault();
              });
              ui.listen(menu, "pointermove", (e4) => {
                if (!view.resizing) return;
                view.manualSize = [Math.max(240, view.resizing.w + e4.screenX - view.resizing.x), Math.max(120, view.resizing.h + e4.screenY - view.resizing.y)];
                menu.style.width = "100%";
                send({ a: "resize", d: window2.devicePixelRatio, w: view.manualSize[0], h: view.manualSize[1] });
              });
              for (const event of ["pointerup", "pointercancel", "lostpointercapture"]) ui.listen(menu, event, () => {
                if (view.resizing) send({ a: "resize-end" });
                view.resizing = null;
              });
              ui.listen(menu, "keydown", (e4) => {
                if (!e4.target.closest(".grip") || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e4.key)) return;
                e4.preventDefault();
                view.manualSize = [Math.max(240, window2.innerWidth + (e4.key === "ArrowRight" ? 10 : e4.key === "ArrowLeft" ? -10 : 0)), Math.max(120, window2.innerHeight + (e4.key === "ArrowDown" ? 10 : e4.key === "ArrowUp" ? -10 : 0))];
                send({ a: "resize", d: window2.devicePixelRatio, w: view.manualSize[0], h: view.manualSize[1] });
                send({ a: "resize-end" });
              });
              ui.listen(document2, "keydown", (e4) => {
                if (e4.key === "Escape" && view.popup) {
                  e4.preventDefault();
                  dashboard.close();
                  return;
                }
                if (e4.key === "Escape") {
                  const overflow = menu.querySelector(".shell-overflow[open]");
                  if (overflow) {
                    overflow.open = false;
                    overflow.querySelector("summary").focus();
                    return;
                  }
                  viewState.flush();
                  send({ a: "hide" });
                }
                if (e4.altKey && e4.key === "ArrowLeft") {
                  e4.preventDefault();
                  if (view.page !== "home" || view.taskFilter || view.working) back();
                }
              });
              ui.listen(menu, "toggle", () => fit(), true);
              ui.effect("ui.fonts", () => document2.fonts.ready, () => {
                if (view.S) fit();
              });
              send({ a: "ready" });
            }
          };
          const { natural, gutter, fit, actionReceipt, actionNotices, focusStart, focusReceipt, focusReadback, remember, navigate: navigate2, back, routeButton, answerSlot, answerNotices, answerToast, tasks, blockProject, unique, mode, counter, known, taskRow, taskOrder, startButton, globalGroup, problem, modeText, admission, incident, incidentNotice, service, when, local, custom, history: history2, update, ai, addProject, bulk, views, appearance, agents, about, setup, attention, projects, controls, home, routeBody, resourceGroup, resourceCapability, resourceReceipt, capacity, workEntry, workPage, mountResources, notifications, notifyGroup, openCard, project, filteredTasks, head, draw, wholeLines, settle, refresh, release, receive, measure, mount } = api;
          function resourceSend(g5, op) {
            const capability = resourceCapability(g5);
            if (!capability.edit) return Promise.resolve({ state: "rejected", reason: capability.reason });
            if (!Array.isArray(op.ops) || !op.ops.length || op.ops.some((o4) => o4.policy_schema !== "taskq.host-capacity/1" || o4.target?.project_id !== null)) return Promise.resolve({ state: "rejected", reason: "Unsupported host capacity scope" });
            const key = g5.key + "#" + op.id;
            if (resourcePayloads.get(g5.key)?.id !== op.id) resourcePayloads.set(g5.key, JSON.parse(JSON.stringify({ a: "capacity", root: g5.key, id: op.id, ops: op.ops, issued_at: view.S.now, expires_at: view.S.now + 300 })));
            return new Promise((resolve) => {
              const prior = resourceRequests.get(key);
              if (prior) cancel(prior.timer);
              resourceRequests.set(key, { resolve, timer: defer(() => {
                resourceRequests.delete(key);
                resolve({ state: "unknown", reason: "no answer from the tray within 20 s; check the limits shown, then retry" });
              }, 2e4) });
              send(resourcePayloads.get(g5.key));
            });
          }
          const window2 = ports.window || globalThis, document2 = ports.document || window2.document, menu = ports.menu || document2?.querySelector(".menu");
          const { defer, cancel, repeat = defer } = ports.timers || ui;
          const TaskqProjectBlocks = require_project_blocks(), TaskqTaskControls = require_task_controls();
          const send = ports.send || ui.send;
          const performance2 = window2.performance || globalThis.performance, crypto2 = window2.crypto || globalThis.crypto, CSS2 = window2.CSS || globalThis.CSS;
          let batching = 0, staged = null;
          const paint = (...args) => {
            if (batching) {
              staged = [!!(staged?.[0] || args[0])];
              return;
            }
            (ports.render || draw)(...args);
          };
          const present = execution.action("ui.render", paint);
          const reports = require_reports().create({ send, draw: () => present(), defer });
          const notificationPanel = require_notification_panel().create({ view, reports, send, draw: () => present() });
          const { tick } = execution.module("ui.render", { tick: paint });
          const renderer = shell.create(menu);
          const color = (st) => ({ doing: "work", review: "review", ask: "ask", failed: "failed", ready: "ready" })[st] || "muted";
          const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
          const f4 = (...children) => h5(Fragment, null, ...children), raw = (html) => html ? h5("div", { style: "display:contents", dangerouslySetInnerHTML: { __html: html } }) : "", lines = (items) => items.flatMap((x5, i4) => i4 ? [h5("br"), x5] : [x5]);
          const runtimeName = (rt) => ({ "claude": "Claude", "codex": "Codex", "codex-app": "Codex app", "antigravity": "Antigravity" })[rt] || rt || "Unknown runtime";
          const family = (rt) => /^codex/.test(rt || "") ? "Codex" : rt === "claude" ? "Claude" : runtimeName(rt), grouped = (list) => {
            const m5 = /* @__PURE__ */ new Map();
            for (const x5 of list) m5.set(x5, (m5.get(x5) || 0) + 1);
            return [...m5].map(([k5, n3]) => n3 > 1 ? k5 + " (" + n3 + ")" : k5).join(" \xB7 ");
          };
          const signState = { "signed-in": "Signed in", expired: "Expired", rejected: "Rejected", "sign-in-required": "Sign in required", "not-installed": "Not installed", "check-failed": "Check failed", unknown: "Unknown" }, signAlert = ["expired", "rejected", "sign-in-required", "check-failed"];
          const actionErrors = /* @__PURE__ */ new Map();
          const routes = { hosts: "Hosts and resources", delivery: "Delivery", "agentq-shown": "Agentq shown", "agentq-setting-up": "Agentq setting up", "agentq-running": "Agentq running", "agentq-unknown": "Agentq unknown", work: "Balancing", recovery: "Recovery", stop: "Stop and hold", home: "Home", projects: "Projects", attention: "Attention", settings: "Settings", appearance: "Appearance", cadence: "Activity history", notifications: "Notifications", "notification-settings": "Notifications", agents: "Agents and access", services: "Services and support", about: "About and updates" };
          const settingsPages = ["settings", "appearance", "hosts", "delivery", "cadence", "notification-settings", "work", "recovery", "agents", "services", "about"];
          if (ports.settings) view.page = "settings";
          const viewState = require_tray_view_state().create(view, { menu, document: document2, send, defer, cancel, trail: renderer.entries(), routes: Object.fromEntries(Object.keys(routes).map((page) => [page, { parent: settingsPages.includes(page) && page !== "settings" ? "settings" : "home" }])) });
          const addKinds = { new: "New project: no taskq.json", plain: "Not a repository: files stay", taskq: "taskq project: selftest passes", old: "Old taskq project: selftest fails" };
          const cutover = "https://github.com/taskqorg/taskq/blob/main/taskq.md#9-config-and-cutover";
          const workPageFor = (p5) => p5.resources?.targets?.some((t4) => t4.supported?.some((s4) => s4.policy_schema === "taskq.host-capacity/1")) ? h5("div", { "data-resources": p5.root }) : h5("p", { role: "status" }, "Legacy capacity editor unavailable; refresh host status.");
          const resourceCards = /* @__PURE__ */ new Map(), resourceRequests = /* @__PURE__ */ new Map(), resourcePayloads = /* @__PURE__ */ new Map();
          const modes = [["off", "Off"], ["silent", "Silent"], ["banner", "Banner"], ["sound", "Banner with sound"]];
          const dashboard = ports.workspace ? null : require_task_dashboard().create(view, { menu, present, send, app: api, owned: !!ports.views, window: window2, document: document2 });
          const startOperations = require_workspace_answers().create(view, { menu, document: document2, send, draw: present, defer, cancel });
          const { answerSend } = startOperations;
          return { ...api, resourceSend };
        }
      });
      module.exports = { create, createState };
    }
  });

  // src/task-workspace.js
  var require_task_workspace = __commonJS({
    "src/task-workspace.js"(exports, module) {
      var tray = require_tray_app();
      module.exports = {
        schema: "taskq.task-workspace/1",
        createState: tray.createState,
        create: tray.create,
        answerCard: require_answer_card(),
        projectBlocks: require_project_blocks(),
        resources: require_resource_controls()
      };
    }
  });
  return require_task_workspace();
})();
/*! PrismJS 1.30.0
MIT LICENSE

Copyright (c) 2012 Lea Verou

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
*/
/*! Bundled license information:

prismjs/components/prism-core.js:
  (**
   * Prism: Lightweight, robust, elegant syntax highlighting
   *
   * @license MIT <https://opensource.org/licenses/MIT>
   * @author Lea Verou <https://lea.verou.me>
   * @namespace
   * @public
   *)
*/
