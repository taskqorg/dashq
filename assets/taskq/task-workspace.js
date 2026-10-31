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
        counters: function(counts, project = "", label = "All projects", partial = [], selected = null, open = null) {
          const total = Number.isSafeInteger(open) && open >= 0 ? open : "?";
          const items = (counts || []).map(({ state, name, n: n3 }) => {
            const [mark, kind] = marks[state] || ["circle", ""];
            if (n3 === 0 && selected === null) return "";
            const value = Number.isSafeInteger(n3) && n3 >= 0 ? n3 : "?";
            const title = label + " \xB7 " + name + ": " + (value === "?" ? "unknown" : value);
            return '<button type="button" class="' + kind + '" data-tqp="tasks" data-project="' + esc(project) + '" data-state="' + state + '"' + (selected === null ? "" : ' aria-pressed="' + selected.includes(state) + '"') + ' aria-label="' + esc(title) + '" title="' + esc(title) + '">' + icon(mark) + (n3 === 0 ? "" : '<span class="tqp-count">' + value + "</span>") + "</button>";
          }).join("");
          return (partial.length ? '<small class="tqp-partial" title="' + esc(partial.join(", ")) + '">Partial: ' + esc(partial.join(", ")) + "</small>" : "") + (items ? '<div class="tqp-counts" aria-label="' + esc(label) + ' counters">' + items + (selected === null ? "" : '<button type="button" data-a="all-tasks" aria-pressed="' + !selected.length + '" aria-label="All tasks: ' + (total === "?" ? "unknown" : total) + ' open" title="All projects \xB7 open tasks">All tasks ' + total + "</button>") + "</div>" : "");
        },
        summary: function(p5, selected = null, open = null) {
          return '<section class="tqp tqp-project" aria-label="' + esc(p5.name) + '"><div class="tqp-identity"><button type="button" data-tqp="project" data-project="' + esc(p5.id) + '">' + icon(p5.pinned ? "pin" : "folder") + " " + esc(p5.name) + "</button>" + link(p5.board.url, icon(p5.board.kind) + " Board", "Open " + p5.name + " Board") + "</div>" + counters(p5.counts, p5.id, p5.name, p5.partial || [], selected, open) + "</section>";
        },
        aggregate: function(all, selected = null) {
          return '<section class="tqp" aria-label="All projects">' + counters(all.counts, "", "All projects", all.partial || [], selected, all.open) + "</section>";
        },
        task: function(p5, row, global = false, compact = false) {
          const name = row.state_name ?? row.state, [mark, kind] = marks[row.state] || ["circle", ""];
          const board = row.key ? row.key.slice(0, row.key.lastIndexOf("/")) : p5.board.url;
          const rt = row.runtime, brand = rt === "claude" ? "claude" : ["codex", "codex-app"].includes(rt) ? "openai" : "";
          const remote = row.host && !row.local_session;
          const badges = (remote ? '<span title="Remote host" aria-label="Remote host">' + icon("network") + "</span> " : "") + (rt ? icon(brand) + " " + esc(rt) : "");
          const target = url(row.session_url, !!row.local_session);
          const session = target ? link(row.session_url, icon("terminal"), "Open " + (rt || "agent") + " session", true) : row.host ? "<small" + (row.session_reason ? ' title="' + esc(row.session_reason) + '"' : "") + ">" + (remote ? "Session unavailable" : "Session starting or unavailable") + "</small>" : "";
          const since = !("since" in row) ? "" : row.age == null ? '<small class="tqp-age" title="Age unknown: no logged state change">age ?</small>' : '<small class="tqp-age" title="Since the last state change">' + esc(row.age) + "</small>";
          const metadata = [since, global ? esc(p5.name) : "", row.host ? esc(row.host) : "", badges].filter(Boolean).join(" \xB7 ");
          const wait = row.wait ? '<small class="tqp-execution-wait" role="status">' + esc(row.wait) + "</small>" : "";
          const parked = (row.stage ? '<small class="tqp-stage" title="First open stage of the Acceptance items">Stage ' + esc(row.stage.k) + "/" + esc(row.stage.n) + "</small>" : "") + (row.until ? '<small class="tqp-parked">' + esc(parkedText(row)) + "</small>" : "");
          if (compact) {
            const elapsed = row.age != null ? '<span class="tqp-age">' + esc(row.age) + "</span>" : '<span class="tqp-age" title="Current stage elapsed unknown">?</span>';
            const label = "View " + (rt || "task") + " agent #" + row.n;
            const runtime = brand ? '<button type="button" class="tqp-mark" data-a="workspace" data-page="answer" data-root="' + esc(row.root || p5.root || "") + '" data-board="' + esc(board) + '" data-task="' + esc(row.n) + '" data-revision="' + esc(row.revision) + '" aria-label="' + esc(label) + '" title="' + esc(label) + '">' + icon(brand) + "</button>" : "";
            const provider = p5.board.kind === "gitlab" ? "GitLab" : p5.board.kind === "github" ? "GitHub" : "recorded board";
            const forge = url(row.url) ? '<a class="task-board" href="' + url(row.url) + '" data-url="' + url(row.url) + '" aria-label="Open #' + esc(row.n) + " on " + provider + '" title="Open #' + esc(row.n) + " on " + provider + '">' + icon(p5.board.kind) + "</a>" : '<span class="task-board" aria-disabled="true" title="Issue target unavailable">' + icon(p5.board.kind) + "</span>";
            const run = (row.host && row.local_session === false ? '<span class="tqp-mark" title="Remote host ' + esc(row.host) + '" aria-label="Remote host">' + icon("network") + "</span>" : "") + runtime;
            return '<div class="tqp tqp-task"><div class="tqp-task-head"><span class="task-stage ' + kind + '" aria-label="' + esc(name) + ' \xB7 current stage" title="' + esc(name) + ' \xB7 current stage">' + (compact && row.state === "ask" ? '<button type="button" class="question-shortcut" data-a="workspace" data-page="answer" data-root="' + esc(row.root || p5.root || "") + '" data-board="' + esc(board) + '" data-revision="' + esc(row.revision) + '" data-task="' + esc(row.n) + '" aria-label="Open question #' + esc(row.n) + '" title="Open question #' + esc(row.n) + '">' + icon(mark) + "</button>" : icon(mark)) + "</span>" + forge + '<button type="button" class="task-identity" data-a="task-detail" data-key="' + esc(row.key) + '" aria-controls="taskq-question-details" aria-expanded="false" aria-label="Details for ' + esc(p5.name) + " #" + row.n + '">' + (global ? esc(p5.name) + " " : "") + "#" + esc(row.n) + "</button>" + (row.state === "review" && row.enqueued ? '<span class="task-publication">' + esc(name) + "</span>" : "") + '<span class="task-when">' + run + elapsed + '</span></div><div class="tqp-task-title">' + link(row.url, esc(row.title), "Open task #" + row.n) + "</div>" + parked + "</div>";
          }
          return '<div class="tqp tqp-task"><div class="tqp-task-head"><span class="' + kind + '" title="' + esc(name) + '" aria-label="' + esc(name) + '">' + icon(mark) + "</span> " + link(row.url, icon(p5.board.kind) + " #" + esc(row.n), "Open task #" + row.n) + ' <span class="tqp-meta">' + metadata + "</span> " + session + " " + wait + '</div><div class="tqp-task-title">' + esc(row.title) + "</div>" + parked + "</div>";
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
      var assets = { "circle-x": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-x"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="m15 9-6 6" />\n  <path d="m9 9 6 6" />\n</svg>\n', "clock": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-clock"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="M12 6v6l4 2" />\n</svg>\n', "folder": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-folder"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />\n</svg>\n', "circle-pause": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-pause"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <line x1="10" x2="10" y1="15" y2="9" />\n  <line x1="14" x2="14" y1="15" y2="9" />\n</svg>\n', "gitlab": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>GitLab</title><path d="m23.6004 9.5927-.0337-.0862L20.3.9814a.851.851 0 0 0-.3362-.405.8748.8748 0 0 0-.9997.0539.8748.8748 0 0 0-.29.4399l-2.2055 6.748H7.5375l-2.2057-6.748a.8573.8573 0 0 0-.29-.4412.8748.8748 0 0 0-.9997-.0537.8585.8585 0 0 0-.3362.4049L.4332 9.5015l-.0325.0862a6.0657 6.0657 0 0 0 2.0119 7.0105l.0113.0087.03.0213 4.976 3.7264 2.462 1.8633 1.4995 1.1321a1.0085 1.0085 0 0 0 1.2197 0l1.4995-1.1321 2.4619-1.8633 5.006-3.7489.0125-.01a6.0682 6.0682 0 0 0 2.0094-7.003z"/></svg>', "git-pull-request": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-git-pull-request"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="18" cy="18" r="3" />\n  <circle cx="6" cy="6" r="3" />\n  <path d="M13 6h3a2 2 0 0 1 2 2v7" />\n  <line x1="6" x2="6" y1="9" y2="21" />\n</svg>\n', "openai": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>OpenAI</title><path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z"/></svg>', "play": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-play"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" />\n</svg>\n', "terminal": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-terminal"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 19h8" />\n  <path d="m4 17 6-6-6-6" />\n</svg>\n', "message-circle-question": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-message-circle-question"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719" />\n  <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />\n  <path d="M12 17h.01" />\n</svg>\n', "github": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>GitHub</title><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>', "circle": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n</svg>\n', "pin": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pin"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 17v5" />\n  <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />\n</svg>\n', "link": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-link"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />\n  <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />\n</svg>\n', "network": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-network"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect x="16" y="16" width="6" height="6" rx="1" />\n  <rect x="2" y="16" width="6" height="6" rx="1" />\n  <rect x="9" y="2" width="6" height="6" rx="1" />\n  <path d="M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3" />\n  <path d="M12 12V8" />\n</svg>\n', "claude": '<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>Claude</title><path d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z"/></svg>' };
      var marks = {
        doing: ["play", "good"],
        review: ["git-pull-request", "review"],
        ask: ["message-circle-question", "ask"],
        failed: ["circle-x", "danger"],
        ready: ["circle", ""],
        waiting: ["link", ""],
        later: ["clock", ""],
        held: ["circle-pause", ""]
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

  // src/ui-icons.json
  var require_ui_icons = __commonJS({
    "src/ui-icons.json"(exports, module) {
      module.exports = { activity: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-activity"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2" />\n</svg>\n', "always-on-top": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-copy taskq-always-on-top"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="14" height="14" x="8" y="8" rx="2" ry="2" fill="currentColor" />\n  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />\n</svg>\n', archive: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-archive"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="20" height="5" x="2" y="3" rx="1" />\n  <path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />\n  <path d="M10 12h4" />\n</svg>\n', "archive-restore": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-archive-restore"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="20" height="5" x="2" y="3" rx="1" />\n  <path d="M4 8v11a2 2 0 0 0 2 2h2" />\n  <path d="M20 8v11a2 2 0 0 1-2 2h-2" />\n  <path d="m9 15 3-3 3 3" />\n  <path d="M12 12v9" />\n</svg>\n', "arrow-down": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-arrow-down"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 5v14" />\n  <path d="m19 12-7 7-7-7" />\n</svg>\n', "arrow-left": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-arrow-left"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m12 19-7-7 7-7" />\n  <path d="M19 12H5" />\n</svg>\n', "arrow-up": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-arrow-up"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m5 12 7-7 7 7" />\n  <path d="M12 19V5" />\n</svg>\n', bell: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-bell"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10.268 21a2 2 0 0 0 3.464 0" />\n  <path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326" />\n</svg>\n', bot: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-bot"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 8V4H8" />\n  <rect width="16" height="12" x="4" y="8" rx="2" />\n  <path d="M2 14h2" />\n  <path d="M20 14h2" />\n  <path d="M15 13v2" />\n  <path d="M9 13v2" />\n</svg>\n', "chart-no-axes-column": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-chart-no-axes-column"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M5 21v-6" />\n  <path d="M12 21V3" />\n  <path d="M19 21V9" />\n</svg>\n', check: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-check"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M20 6 9 17l-5-5" />\n</svg>\n', "check-check": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-check-check"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M18 6 7 17l-5-5" />\n  <path d="m22 10-7.5 7.5L13 16" />\n</svg>\n', "circle-alert": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-alert"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <line x1="12" x2="12" y1="8" y2="12" />\n  <line x1="12" x2="12.01" y1="16" y2="16" />\n</svg>\n', "circle-check": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-check"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="m9 12 2 2 4-4" />\n</svg>\n', "circle-question-mark": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-question-mark"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />\n  <path d="M12 17h.01" />\n</svg>\n', "circle-slash": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-circle-slash"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <line x1="9" x2="15" y1="15" y2="9" />\n</svg>\n', copy: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-copy"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />\n  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />\n</svg>\n', download: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-download"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 15V3" />\n  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />\n  <path d="m7 10 5 5 5-5" />\n</svg>\n', ellipsis: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-ellipsis"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="1" />\n  <circle cx="19" cy="12" r="1" />\n  <circle cx="5" cy="12" r="1" />\n</svg>\n', "external-link": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-external-link"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15 3h6v6" />\n  <path d="M10 14 21 3" />\n  <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />\n</svg>\n', "file-plus-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-file-plus-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M11.35 22H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.706.706l3.588 3.588A2.4 2.4 0 0 1 20 8v5.35" />\n  <path d="M14 2v5a1 1 0 0 0 1 1h5" />\n  <path d="M14 19h6" />\n  <path d="M17 16v6" />\n</svg>\n', "file-text": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-file-text"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" />\n  <path d="M14 2v5a1 1 0 0 0 1 1h5" />\n  <path d="M10 9H8" />\n  <path d="M16 13H8" />\n  <path d="M16 17H8" />\n</svg>\n', focus: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-focus"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="3" />\n  <path d="M3 7V5a2 2 0 0 1 2-2h2" />\n  <path d="M17 3h2a2 2 0 0 1 2 2v2" />\n  <path d="M21 17v2a2 2 0 0 1-2 2h-2" />\n  <path d="M7 21H5a2 2 0 0 1-2-2v-2" />\n</svg>\n', "folder-open": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-folder-open"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />\n</svg>\n', "git-merge": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-git-merge"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="18" cy="18" r="3" />\n  <circle cx="6" cy="6" r="3" />\n  <path d="M6 21V9a9 9 0 0 0 9 9" />\n</svg>\n', "git-pull-request": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-git-pull-request"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="18" cy="18" r="3" />\n  <circle cx="6" cy="6" r="3" />\n  <path d="M13 6h3a2 2 0 0 1 2 2v7" />\n  <line x1="6" x2="6" y1="9" y2="21" />\n</svg>\n', grip: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-grip"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="5" r="1" />\n  <circle cx="19" cy="5" r="1" />\n  <circle cx="5" cy="5" r="1" />\n  <circle cx="12" cy="12" r="1" />\n  <circle cx="19" cy="12" r="1" />\n  <circle cx="5" cy="12" r="1" />\n  <circle cx="12" cy="19" r="1" />\n  <circle cx="19" cy="19" r="1" />\n  <circle cx="5" cy="19" r="1" />\n</svg>\n', history: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-history"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />\n  <path d="M3 3v5h5" />\n  <path d="M12 7v5l4 2" />\n</svg>\n', house: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-house"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8" />\n  <path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />\n</svg>\n', inbox: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-inbox"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />\n  <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />\n</svg>\n', info: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-info"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <circle cx="12" cy="12" r="10" />\n  <path d="M12 16v-4" />\n  <path d="M12 8h.01" />\n</svg>\n', "list-filter": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-list-filter"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M2 5h20" />\n  <path d="M6 12h12" />\n  <path d="M9 19h6" />\n</svg>\n', "list-ordered": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-list-ordered"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M11 5h10" />\n  <path d="M11 12h10" />\n  <path d="M11 19h10" />\n  <path d="M4 4h1v5" />\n  <path d="M4 9h2" />\n  <path d="M6.5 20H3.4c0-1 2.6-1.925 2.6-3.5a1.5 1.5 0 0 0-2.6-1.02" />\n</svg>\n', "maximize-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-maximize-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15 3h6v6" />\n  <path d="m21 3-7 7" />\n  <path d="m3 21 7-7" />\n  <path d="M9 21H3v-6" />\n</svg>\n', "messages-square": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-messages-square"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M16 10a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 14.286V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />\n  <path d="M20 9a2 2 0 0 1 2 2v10.286a.71.71 0 0 1-1.212.502l-2.202-2.202A2 2 0 0 0 17.172 19H10a2 2 0 0 1-2-2v-1" />\n</svg>\n', "minimize-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-minimize-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m14 10 7-7" />\n  <path d="M20 10h-6V4" />\n  <path d="m3 21 7-7" />\n  <path d="M4 14h6v6" />\n</svg>\n', "octagon-x": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-octagon-x"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m15 9-6 6" />\n  <path d="M2.586 16.726A2 2 0 0 1 2 15.312V8.688a2 2 0 0 1 .586-1.414l4.688-4.688A2 2 0 0 1 8.688 2h6.624a2 2 0 0 1 1.414.586l4.688 4.688A2 2 0 0 1 22 8.688v6.624a2 2 0 0 1-.586 1.414l-4.688 4.688a2 2 0 0 1-1.414.586H8.688a2 2 0 0 1-1.414-.586z" />\n  <path d="m9 9 6 6" />\n</svg>\n', "panel-left-close": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-panel-left-close"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n  <path d="M9 3v18" />\n  <path d="m16 15-3-3 3-3" />\n</svg>\n', "panels-top-left": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-panels-top-left"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n  <path d="M3 9h18" />\n  <path d="M9 21V9" />\n</svg>\n', paperclip: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-paperclip"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551" />\n</svg>\n', pause: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pause"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect x="14" y="3" width="5" height="18" rx="1" />\n  <rect x="5" y="3" width="5" height="18" rx="1" />\n</svg>\n', pin: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pin"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 17v5" />\n  <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />\n</svg>\n', "pin-off": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-pin-off"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 17v5" />\n  <path d="M15 9.34V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H7.89" />\n  <path d="m2 2 20 20" />\n  <path d="M9 9v1.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h11" />\n</svg>\n', plus: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-plus"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M5 12h14" />\n  <path d="M12 5v14" />\n</svg>\n', "rotate-ccw": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-rotate-ccw"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />\n  <path d="M3 3v5h5" />\n</svg>\n', "rotate-cw": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-rotate-cw"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />\n  <path d="M21 3v5h-5" />\n</svg>\n', "rows-3": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-rows-3"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n  <path d="M21 9H3" />\n  <path d="M21 15H3" />\n</svg>\n', save: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-save"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />\n  <path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7" />\n  <path d="M7 3v4a1 1 0 0 0 1 1h7" />\n</svg>\n', search: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-search"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m21 21-4.34-4.34" />\n  <circle cx="11" cy="11" r="8" />\n</svg>\n', settings: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-settings"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915" />\n  <circle cx="12" cy="12" r="3" />\n</svg>\n', shield: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-shield"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />\n</svg>\n', "sliders-horizontal": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-sliders-horizontal"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10 5H3" />\n  <path d="M12 19H3" />\n  <path d="M14 3v4" />\n  <path d="M16 17v4" />\n  <path d="M21 12h-9" />\n  <path d="M21 19h-5" />\n  <path d="M21 5h-7" />\n  <path d="M8 10v4" />\n  <path d="M8 12H3" />\n</svg>\n', square: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-square"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <rect width="18" height="18" x="3" y="3" rx="2" />\n</svg>\n', "trash-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-trash-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M10 11v6" />\n  <path d="M14 11v6" />\n  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />\n  <path d="M3 6h18" />\n  <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />\n</svg>\n', "triangle-alert": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-triangle-alert"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />\n  <path d="M12 9v4" />\n  <path d="M12 17h.01" />\n</svg>\n', type: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-type"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M12 4v16" />\n  <path d="M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2" />\n  <path d="M9 20h6" />\n</svg>\n', "undo-2": '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-undo-2"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M9 14 4 9l5-5" />\n  <path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11" />\n</svg>\n', x: '<!-- @license lucide-static v1.17.0 - ISC -->\n<svg\n  class="lucide lucide-x"\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n  fill="none"\n  stroke="currentColor"\n  stroke-width="2"\n  stroke-linecap="round"\n  stroke-linejoin="round"\n>\n  <path d="M18 6 6 18" />\n  <path d="m6 6 12 12" />\n</svg>\n' };
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
          if (JSON.stringify(local.draft.base) !== JSON.stringify(revs(group))) return "conflict";
          return local.draft.review ? "review" : "editing";
        },
        step: function(group, local, cap, ev, newId, now) {
          const st = status(group, local, cap, now), l5 = { ...local };
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
              if (st === "conflict") l5.draft = { ...l5.draft, base: revs(group), review: false };
              return { local: l5, send: null };
            case "cancel":
              return ["editing", "review", "conflict"].includes(st) ? { local: fresh(), send: null } : { local, send: null };
            case "confirm": {
              if (st !== "review") return { local, send: null };
              l5.op = { id: newId(), ops: changes(group, l5.draft) };
              l5.receipt = { state: "sending", from: revs(group) };
              l5.draft = null;
              return { local: l5, send: l5.op };
            }
            case "receipt":
              if (!l5.op || ev.opId !== l5.op.id) return { local, send: null };
              l5.receipt = { ...l5.receipt, state: ["saved", "rejected", "unknown"].includes(ev.state) ? ev.state : "unknown", reason: ev.reason, saved_revision: ev.saved_revision };
              return { local: l5, send: null };
            case "retry":
              return st === "unknown" ? { local: { ...l5, receipt: { ...l5.receipt, state: "sending", reason: null } }, send: l5.op } : { local, send: null };
            case "dismiss":
              return ["applied", "rejected", "superseded"].includes(st) ? { local: fresh(), send: null } : { local, send: null };
            default:
              return { local, send: null };
          }
        },
        render: function(group, local, cap, now) {
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
          let h5 = '<section class="rsc rsc-work" data-rsc-state="' + st + '" aria-label="' + esc("Work mode on " + (host || "this host")) + '">';
          h5 += '<p class="rsc-row"><span>Work mode</span><span class="rsc-muted">' + esc((host || "this host") + " \xB7 selected projects") + "</span></p>";
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
          if (st === "view" || st === "read-only") return h5 + '<button type="button" data-rsc="open"' + (st === "view" ? "" : " disabled") + ">Edit work mode</button>" + (st === "read-only" ? '<p class="rsc-muted">' + (sel.length ? "A selected project is read-only here" : "Select projects to change") + "</p>" : "") + "</section>";
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

  // src/reports.js
  var require_reports = __commonJS({
    "src/reports.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      var { create, body } = execution.module("reports", {
        create: function({ send = ui.send, draw, defer = ui.defer, id = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), (b5) => b5.toString(16).padStart(2, "0")).join("") }) {
          let expanded = false, projects = [], pages = /* @__PURE__ */ new Map(), pending = /* @__PURE__ */ new Map(), selected = null, notice = "", full = false, draft = null, successor = null, readFailure = "", readQueue = [];
          const receipts = /* @__PURE__ */ new Map(), outcomes = /* @__PURE__ */ new Map(), key = (root, n3) => root + "#" + n3;
          const receiptKey = (root, n3, sha) => key(root, n3) + "@" + sha;
          const active = (op) => selected?.root === op.root && selected.n === op.n && selected.sha === op.sha;
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
          function request(root, action, extra = {}) {
            if ([...pending.values()].some((p5) => p5.root === root && p5.n === extra.n && p5.sha === extra.sha && (action === "read" || p5.action !== "read"))) return;
            const op = { id: id(), root, action, ...extra };
            pending.set(op.id, op);
            send({ a: "reports", ...op });
            defer(() => {
              if (pending.delete(op.id)) {
                if (op.action === "read") readFailure = "Result read outcome unknown; waiting for service readback.";
                outcome(op, "Result outcome unknown; waiting for service readback.");
                api.readNext();
                draw();
              }
            }, 18e4);
          }
          function summary(text, heading) {
            const lines = (text || "").split("\n"), start = lines.findIndex((l5) => l5.replace(/^#+\s*/, "").trim().toLowerCase() === heading.toLowerCase());
            if (start < 0) return "";
            const rest = lines.slice(start + 1), end = rest.findIndex((l5) => /^#{1,6}\s/.test(l5));
            return rest.slice(0, end < 0 ? void 0 : end).join("\n").trim();
          }
          function field(text, label) {
            const lines = (text || "").split("\n"), start = lines.findIndex((l5) => l5.startsWith(label + ":"));
            if (start < 0) return "Unknown: not recorded in the accepted hand-in.";
            const rest = lines.slice(start + 1), end = rest.findIndex((l5) => /^(Changed|Checks|Open|Report):/.test(l5) && !l5.startsWith(label + ":"));
            return [lines[start], ...rest.slice(0, end < 0 ? void 0 : end)].map((l5) => l5.startsWith(label + ":") ? l5.slice(label.length + 1).trim() : l5).join("\n");
          }
          function remember(op, report) {
            const k5 = receiptKey(op.root, op.n, op.sha), won = receipts.get(k5);
            if (won?.sha === op.sha && won.visible === false) return won;
            const fields = { sha: report.sha };
            for (const field2 of ["read", "decision", "visible", "pending_decision", "tasks_made", "comments_through", "later_until"]) if (Object.hasOwn(report, field2)) fields[field2] = report[field2];
            if (op.action === "read" && won?.sha === op.sha) {
              if (["later", "accepted"].includes(won.decision)) fields.decision = won.decision;
              if (won.pending_decision === "make") fields.pending_decision = "make";
            }
            if (Number.isSafeInteger(won?.comments_through) && Number.isSafeInteger(fields.comments_through)) fields.comments_through = Math.max(won.comments_through, fields.comments_through);
            const merged = { ...won?.sha === op.sha ? won : {}, ...fields };
            receipts.set(k5, merged);
            return merged;
          }
          const api = execution.module("reports.view", {
            sync(ps) {
              projects = targets(ps);
              for (const p5 of projects) {
                const data = p5.reports;
                pages.set(p5.root, { rows: (data?.reports || []).flatMap((r4) => {
                  const k5 = receiptKey(p5.root, r4.n, r4.sha), won = receipts.get(k5);
                  if (Number.isSafeInteger(r4.comments_through) && Number.isSafeInteger(won?.comments_through) && r4.comments_through >= won.comments_through) receipts.delete(k5);
                  const row = receipts.has(k5) ? { ...r4, ...won } : r4;
                  return row.visible === false ? [] : [row];
                }), unknown: data?.complete !== true, reason: data?.reason || (data?.complete === true ? "" : "Reports not collected yet") });
              }
              if (selected) {
                const row = api.rows().find((r4) => key(r4.root, r4.n) === key(selected.root, selected.n) && r4.sha === selected.sha);
                selected = row ? { ...selected, ...row } : null;
              }
            },
            toggle(ps) {
              return execution.action("reports.list.open", () => {
                api.sync(ps);
                expanded = !expanded;
                draw();
              })();
            },
            refresh() {
              api.sync(projects);
              draw();
            },
            rows() {
              return projects.flatMap((p5) => (pages.get(p5.root)?.rows || []).map((r4) => ({ ...r4, root: p5.root }))).sort((a4, b5) => (b5.closed || 0) - (a4.closed || 0) || b5.n - a4.n);
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
            open(root, n3) {
              return execution.action("reports.card.open", () => {
                const r4 = api.rows().find((r5) => r5.root === root && r5.n === n3);
                if (!r4) return;
                selected = { ...r4 };
                expanded = true;
                full = false;
                const prior = outcomes.get(receiptKey(root, n3, r4.sha));
                notice = prior?.notice || "";
                draft = null;
                successor = prior?.successor || null;
                draw();
                if ((r4.body || r4.kind === "task") && !r4.read) request(root, "read", { n: n3, sha: r4.sha });
              })();
            },
            move(step) {
              const rows = api.rows(), i4 = rows.findIndex((r5) => r5.root === selected?.root && r5.n === selected?.n), r4 = rows[i4 + step];
              if (r4) api.open(r4.root, r4.n);
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
              pending.delete(r4.id);
              const shown = active(op);
              if (r4.reason) {
                if (op.action === "read") readFailure = r4.reason;
                if (r4.report?.n === op.n && r4.report.sha === op.sha) remember(op, r4.report);
                const made = Array.isArray(r4.receipts) ? r4.receipts : [], uncertain = r4.receipt?.task;
                outcome(op, r4.reason + (made.length ? " \xB7 Created held drafts: " + made.map((n3) => "#" + n3).join(", ") : "") + (uncertain > 0 && !made.includes(uncertain) ? " \xB7 Unverified draft: #" + uncertain : ""), void 0, shown);
                api.readNext();
                draw();
                return true;
              }
              if (r4.report?.n === op.n && r4.report.sha === op.sha) {
                const p5 = pages.get(r4.root), report = remember(op, r4.report);
                if (p5) p5.rows = p5.rows.flatMap((row) => row.n !== op.n || row.sha !== op.sha ? [row] : report.visible === false ? [] : [{ ...row, ...report, closed: row.closed }]);
                if (selected && selected.sha === op.sha && key(selected.root, selected.n) === key(r4.root, op.n)) selected = report.visible === false ? null : { ...selected, ...report, closed: selected.closed };
                if (op.action === "later" && shown && draft === "later") draft = null;
                if (op.action !== "read") outcome(op, report.visible === false ? report.decision === "made" ? "Created held drafts: " + (report.tasks_made || []).map((n3) => "#" + n3).join(", ") : "Report dropped." : op.action === "later" ? "Later: back in Decide on " + (report.later_until || op.until || "").slice(0, 10) + "." : "", void 0, shown);
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
              return true;
            },
            click(b5, ps) {
              const action = b5.dataset.report;
              if (!action) return false;
              if (action === "toggle") api.toggle(ps);
              else if (action === "refresh") api.refresh();
              else if (action === "open") api.open(b5.dataset.root, Number(b5.dataset.n));
              else if (action === "full") {
                full = !full;
                draw();
              } else if (action === "back") {
                selected = null;
                draw();
              } else if (action === "previous") api.move(-1);
              else if (action === "next") api.move(1);
              else if (["followup", "reopen", "later"].includes(action)) {
                draft = action;
                draw();
              } else if (action === "later-save") {
                const day = b5.closest(".report-card")?.querySelector("[data-later-until]")?.value, at = day && /* @__PURE__ */ new Date(day + "T12:00:00Z");
                if (at > Date.now()) api.act("later", void 0, at.toISOString().replace(/\.\d{3}Z$/, "Z"));
                else {
                  notice = "Later needs a future revisit date.";
                  draw();
                }
              } else if (action === "create") {
                const text = b5.closest(".report-card")?.querySelector("[data-result-reason]")?.value;
                api.act(draft, text);
              } else if (["accept", "make", "drop"].includes(action)) api.act(action);
              return true;
            },
            threads() {
              return api.rows().map((r4) => ({ id: "task:" + (projects.find((p5) => p5.root === r4.root)?.issues || r4.root) + "/" + r4.n, root: r4.root, report: r4.n, title: r4.title, result: r4.result, updated: r4.closed * 1e3, status: r4.kind === "task" ? "Finished" : "Report", marker: api.undecided(r4) ? "Needs decision" : r4.read ? "" : r4.kind === "task" ? "New result" : "" }));
            },
            section(ps, reader = false) {
              api.sync(ps);
              const loaded = api.count() !== null, rows = api.rows(), busy = selected && [...pending.values()].some((p5) => p5.root === selected.root && p5.n === selected.n && p5.action !== "read"), project = (root) => projects.find((p5) => p5.root === root)?.name || "Project unavailable";
              const ordinary = selected?.kind === "task", position = rows.findIndex((r4) => r4.root === selected?.root && r4.n === selected?.n), decide = api.decide(rows), age = (r4) => r4.closed ? Math.max(0, Math.floor((Date.now() / 1e3 - r4.closed) / 86400)) + "d" : "age unknown";
              const day = (t4) => new Date(t4).toISOString().slice(0, 10), row = (r4, d5) => h5("button", { class: "project report-row", "data-report": "open", "data-root": r4.root, "data-n": r4.n, key: key(r4.root, r4.n) }, h5("span", { class: "meta" }, project(r4.root) + " \xB7 #" + r4.n + " \xB7 " + (r4.read ? "Read" : "Unread") + (d5 ? " \xB7 Decide \xB7 " + (r4.experiment_verdict || "verdict unknown") + " \xB7 " + age(r4) : "")), h5("span", null, r4.title));
              return h5(
                "section",
                { class: "reports-section group", "aria-label": "Results", key: "reports" },
                !reader && h5("button", { class: "project", type: "button", "data-report": "toggle", "aria-expanded": String(expanded) }, "Results \xB7 " + (loaded ? rows.length : "unknown"), api.unread().length ? " \xB7 " + api.unread().length + " new" : ""),
                expanded && h5(
                  "div",
                  { class: "reports-content body" },
                  selected && h5("div", { class: "options" }, h5("button", { "data-report": "back" }, reader ? "All events" : "All results"), h5("button", { "data-report": "previous", disabled: position <= 0 }, "Previous"), h5("button", { "data-report": "next", disabled: position < 0 || position >= rows.length - 1 }, "Next")),
                  notice && h5("p", { role: "status" }, notice),
                  successor && h5(successor.url ? "a" : "span", { href: successor.url, "data-url": successor.url }, "Read held draft #" + successor.n),
                  projects.map((p5) => pages.get(p5.root)?.reason && h5("p", { role: "status", key: p5.root }, p5.name + ": " + pages.get(p5.root).reason)),
                  selected ? h5(
                    "article",
                    { class: "report-card", "aria-label": ordinary ? "Completed result" : "Research report" },
                    h5("h2", null, selected.title),
                    h5("p", { class: "meta" }, project(selected.root) + " \xB7 ", h5("a", { href: selected.url, "data-url": selected.url }, "#" + selected.n), " \xB7 " + (selected.read ? "Read" : "Unread")),
                    selected.reason && h5("p", { role: "status" }, "Result unknown: " + selected.reason),
                    ordinary ? [
                      ...["Changed", "Checks", "Open"].map((label) => h5("div", { key: label }, h5("h3", null, label), body(field(selected.body, label)))),
                      selected.pr && h5("a", { href: selected.pr, "data-url": selected.pr }, "Open PR"),
                      selected.verdict && h5("details", null, h5("summary", null, "Review and delivery"), body(selected.verdict)),
                      h5("div", { class: "options" }, ["accept", "followup", "reopen"].map((action) => h5("button", { "data-report": action, disabled: !!busy || action === "accept" && selected.decision === "accepted" }, { accept: selected.decision === "accepted" ? "Accepted" : "Accept", followup: "Follow-up task", reopen: "Reopen" }[action]))),
                      draft && h5("div", { key: draft }, h5("label", null, (draft === "reopen" ? "Reopen" : "Follow-up") + " reason", h5("input", { "data-result-reason": "", maxLength: 180, required: true, "aria-label": "Reason", placeholder: "What needs to change?" })), h5("p", { class: "meta" }, "Creates a linked held successor. The original stays closed. Read the draft back before starting it."), h5("button", { "data-report": "create", disabled: !!busy }, "Create held draft"))
                    ] : [
                      selected.body ? [h5("h3", null, "Question"), body(summary(selected.body, "Question")), h5("h3", null, "Recommendation"), body(summary(selected.body, "Recommendation"))] : h5("p", null, "Report content unknown; waiting for background collection."),
                      selected.tasks && h5("div", null, h5("h3", null, "Proposed tasks"), selected.tasks.length ? selected.tasks.map((t4, i4) => h5("details", { key: i4 }, h5("summary", null, i4 + 1 + ". " + t4.title), h5("p", null, t4.goal), h5("ul", null, t4.acceptance?.map((a4) => h5("li", null, a4))), t4.scope?.map((s4) => h5("p", null, s4)), h5("p", { class: "meta" }, [t4.type, t4.runtime, t4.priority && "Priority " + t4.priority].filter(Boolean).join(" \xB7 ")))) : h5("p", null, "No follow-up tasks proposed.")),
                      h5("p", { class: "meta" }, "Make tasks creates held drafts linked to this report. Start them separately."),
                      selected.pending_decision === "make" && h5("p", { role: "status" }, "Make tasks is pending. Retry to reconcile the held drafts."),
                      h5("div", { class: "options" }, ["make", "later", "drop"].map((action) => h5("button", { "data-report": action, disabled: !!busy || !selected.body || action === "make" && !selected.tasks?.length || action === "drop" && selected.pending_decision === "make" }, { make: selected.pending_decision === "make" ? "Retry Make tasks" : "Make tasks", later: "Later", drop: "Drop" }[action]))),
                      draft === "later" && h5("div", { key: "later" }, h5("label", null, "Revisit on", h5("input", { type: "date", "data-later-until": "", min: day(Date.now() + 864e5), value: day(Date.now() + 7 * 864e5), "aria-label": "Revisit date" })), h5("p", { class: "meta" }, "The report returns to Decide on this date."), h5("button", { "data-report": "later-save", disabled: !!busy }, "Keep until this date")),
                      selected.body && h5("div", null, h5("button", { "data-report": "full", "aria-expanded": String(full) }, full ? "Hide full report" : "Full report"), full && h5("div", { class: "report-body" }, body(selected.body)))
                    ]
                  ) : [decide.length ? [h5("h3", { key: "decide" }, "Decide \xB7 " + decide.length), ...decide.map((r4) => row(r4, true))] : null, rows.length ? rows.filter((r4) => !decide.includes(r4)).map((r4) => row(r4, false)) : loaded && h5("p", null, "No results waiting for review.")]
                )
              );
            }
          });
          return api;
        },
        // Keep untrusted Markdown as text/vnodes; evidence URLs are the only active content. No raw HTML.
        body: function(text) {
          const prose = text.replace(/```taskq-tasks\n[\s\S]*?```/g, "").replace(/(^|\n)#{1,6}[ \t]+Proposed tasks[ \t]*(?=\n|$)/gi, "$1").replace(/(^|\n)(#{1,6} [^\n]+)\n/g, "$1$2\n\n");
          return prose.split(/\n\s*\n/).map((p5, i4) => {
            const heading = /^#{1,6}\s+(.+)$/.exec(p5.trim());
            if (heading) return h5("h3", { key: i4 }, heading[1]);
            const parts = p5.split(/(\[[^\]\n]+\]\(https?:\/\/[^\s)]+\)|https?:\/\/[^\s<>\])]+)/g).map((s4, k5) => {
              const link = /^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/.exec(s4), url = link?.[2] || (/^https?:\/\//.test(s4) ? s4 : null);
              return url ? h5("a", { key: k5, href: url, "data-url": url }, link?.[1] || url) : s4;
            });
            return h5("p", { key: i4, style: { whiteSpace: "pre-wrap", overflowWrap: "anywhere" } }, parts);
          });
        }
      });
      module.exports = { create };
    }
  });

  // src/notification-panel.js
  var require_notification_panel = __commonJS({
    "src/notification-panel.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var { icon } = require_ui_icons2();
      var { execution } = require_ui_runtime();
      var kinds = { ask: "Question", close: "Finished", failed: "Failed", gone: "Attempt ended", blocked: "Blocked" };
      var api = execution.module("notificationPanel", {
        // M14: result SHA owns result reads; local event markers never become queue/decision state.
        rows: function(projects = [], results = [], marks = {}) {
          const seen = /* @__PURE__ */ new Set(), rows = [], ids = /* @__PURE__ */ new Set(), closed = new Set(results.map((r4) => r4.root + "#" + r4.n));
          for (const p5 of projects) {
            const board = p5.issues || p5.root;
            if (seen.has(board)) continue;
            seen.add(board);
            const add = (r4) => {
              r4.id = JSON.stringify([board, r4.source, r4.n, r4.source === "result" ? r4.sha : [r4.at, r4.kind, r4.event]]);
              r4.read = r4.source === "result" ? !!r4.read : marks[r4.id] === "read" || marks[r4.id] === "cleared";
              if (marks[r4.id] !== "cleared" && !ids.has(r4.id)) {
                ids.add(r4.id);
                rows.push(r4);
              }
            };
            for (const r4 of results.filter((r5) => r5.root === p5.root)) add({ ...r4, source: "result", resultKind: r4.kind, kind: "close", at: r4.closed, project: p5.name });
            for (const e4 of p5.events || []) {
              if (!kinds[e4[1]] || e4[1] === "close" && closed.has(p5.root + "#" + e4[3])) continue;
              const task = (p5.tasks || []).find((t4) => t4.n === e4[3]), result = results.find((r4) => r4.root === p5.root && r4.n === e4[3]);
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
        create: function({ view, reports, send, draw, openCard }) {
          let notice = "";
          const model = execution.module("notificationPanel.view", {
            rows() {
              return api.rows(view.S.projects || [], reports.rows(), view.S.notify?.inbox || {});
            },
            reasons() {
              return [.../* @__PURE__ */ new Set([...reports.count() === null ? ["Results incomplete; check result collection."] : [], ...(view.S.projects || []).flatMap((p5) => p5.events_error || p5.alive === false ? [p5.name + ": " + (p5.events_error || "project observation stale")] : [])])];
            },
            label() {
              const n3 = model.rows().filter((r4) => !r4.read).length;
              return "Notifications \xB7 " + n3 + (model.reasons().length ? " loaded unread \xB7 history incomplete" : " unread");
            },
            bell() {
              const n3 = model.rows().filter((r4) => !r4.read).length, label = model.label();
              return h5("button", { class: "notifications-entry", type: "button", "data-route": "notifications", title: label, "aria-label": label }, icon("bell"), n3 > 0 && h5("span", { class: "badge", "aria-hidden": "true" }, n3 > 99 ? "99+" : String(n3)), model.reasons().length > 0 && h5("span", { class: "notification-warning", "aria-hidden": "true" }, icon("circle-alert")));
            },
            save(action, rows) {
              if (rows.length) send({ a: "notification-state", action, ids: rows.map((r4) => r4.id) });
            },
            read(rows) {
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
              if (action === "read") model.read(rows);
              else if (action === "clear") model.save("clear", rows.filter((r4) => r4.read));
              else if (action === "open") {
                const row = rows.find((r4) => r4.id === b5.dataset.id);
                if (row) {
                  if (row.source === "result") reports.open(row.root, row.n);
                  else {
                    model.read([row]);
                    openCard({ ...row, title: row.title });
                  }
                }
              }
              return true;
            },
            body() {
              const rows = model.rows(), reasons = model.reasons(), row = (r4) => h5("button", { class: "project notification-row" + (!r4.read ? " unread" : ""), "data-notification": "open", "data-id": r4.id, key: r4.id }, h5("span", { class: "meta" }, (r4.read ? "Read" : "Unread") + " \xB7 " + (r4.source === "result" && r4.resultKind !== "task" ? "Report ready" : kinds[r4.kind]) + " \xB7 " + r4.project + " \xB7 #" + r4.n), h5("span", null, r4.title));
              return h5(
                "section",
                { "aria-label": "Notifications", class: "notification-panel" },
                h5("div", { class: "options" }, h5("button", { "data-notification": "read", disabled: !rows.some((r4) => !r4.read) }, "Mark all read"), h5("button", { "data-notification": "clear", disabled: !rows.some((r4) => r4.read) }, "Clear read"), h5("button", { "data-route": "notification-settings" }, "Delivery settings")),
                notice && h5("p", { role: "alert" }, notice),
                !reports.current() && reports.feedback() && h5("p", { role: "status" }, reports.feedback()),
                reports.readFailure() && h5("p", { role: "status" }, reports.readFailure()),
                reasons.map((reason) => h5("p", { role: "status", class: "meta", key: reason }, reason)),
                reports.current() ? reports.section(view.S.projects, true) : rows.length ? api.groups(rows).map((g5) => g5.rows.length === 1 ? row(g5.rows[0]) : h5("details", { class: "notification-series", "data-notification-group": "", "data-view-key": "notification-series:" + g5.rows[0].id, key: g5.rows[0].id }, h5("summary", null, g5.title + " \xB7 " + g5.rows.length + " events \xB7 " + g5.rows.filter((r4) => !r4.read).length + " unread"), g5.rows.map(row))) : h5("p", null, reasons.length ? "No loaded notifications. History incomplete." : "No notifications.")
              );
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
          return { schema: 1, ...root === "home" ? { dashboard: require_task_views().normalize(saved.dashboard, saved.current) } : {}, current: entry(saved.current), trail: (Array.isArray(saved.trail) ? saved.trail : []).slice(-32).filter((v4) => v4 && entry(v4).page === v4.page).map(entry) };
        },
        create: function(view, { menu, document: document2, send, defer, cancel, routes, trail, root = "home" }) {
          let hydrated = false, timer = null, last = "", restoring = false;
          const api = execution.module("tray.viewState.instance", {
            groups: function() {
              return Object.fromEntries([...menu.querySelectorAll("details[data-view-key]:not(.shell-overflow)")].map((d5) => [d5.dataset.viewKey, d5.open]));
            },
            capture: function() {
              const active = document2.activeElement?.closest?.(".shell-overflow")?.querySelector("summary") || document2.activeElement, key = active && JSON.stringify(active.dataset), focus = active && menu.contains?.(active) ? { tag: active.tagName, key, index: [...menu.querySelectorAll(active.tagName)].filter((e4) => JSON.stringify(e4.dataset) === key).indexOf(active) } : null;
              return { page: view.page, operation: view.operation, project: view.S?.projects?.some((p5) => p5.root === view.page) ? view.page : null, parent: routes[view.page]?.parent || root, taskFilter: view.taskFilter, working: view.working, aiRoot: view.aiRoot, answer: view.answer, messageFilter: view.messageFilter, message: view.message, section: view.section, scroll: (menu.querySelector(".content") || document2.scrollingElement)?.scrollTop || 0, open: api.groups(), focus, unavailable: view.unavailable };
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
              const navigation = normalize({ schema: 1, current: api.capture(), trail, ...root === "home" ? { dashboard: view.dashboard } : {} }, view.S, routes, root), text = JSON.stringify(navigation);
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
        // P-W4: `owned` is the View window surface; the menu sends `views-open` when native offers that window, else keeps the overlay.
        create: function(view, { menu, present, send, app, owned = false, window: window2 = globalThis, document: document2 = window2.document }) {
          const crypto2 = window2.crypto || globalThis.crypto, CSS2 = window2.CSS || globalThis.CSS;
          let restore = true, requests = 0;
          const pinReads = /* @__PURE__ */ new Map();
          let transcript = null;
          const api = execution.module("taskDashboard.instance", {
            model: function() {
              const m5 = view.dashboard || (view.dashboard = views.normalize(null, { taskFilter: view.taskFilter }));
              m5.collapsed = m5.collapsed.filter((id) => view.S.projects.some((p5) => views.identity(p5) === id));
              return m5;
            },
            current: function() {
              return views.current(api.model());
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
            anchor: function(target) {
              const r4 = target.getBoundingClientRect();
              return { left: r4.left, top: r4.top, bottom: r4.bottom };
            },
            // MP13: details open only from click, Enter or Space on the identity (native button activation); `focus` moves focus into them.
            open: function(kind, target, key = null, focus = true) {
              view.popup = { kind, key, anchor: api.anchor(target) };
              if (kind === "task") api.current().open = key;
              present();
              if (focus) menu.querySelector("#taskq-question-details,.task-transcript-page,#taskq-popover")?.focus({ preventScroll: true });
            },
            close: function(focus = true) {
              const p5 = view.popup;
              if (!p5) return;
              view.popup = null;
              if (p5.kind === "task") api.current().open = null;
              if (p5.kind === "transcript") view.restoreScroll = transcript?.returnScroll || 0;
              present();
              if (focus) {
                const target = p5.kind === "task" ? menu.querySelector('[data-a="task-detail"][data-key="' + CSS2.escape(p5.key) + '"]') : p5.kind === "transcript" ? menu.querySelector('[data-a="completed-transcript"][data-ref="' + CSS2.escape(p5.key) + '"]') : menu.querySelector('[data-a="' + (p5.kind === "views" ? "view-list" : "view-editor") + '"]');
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
            readTranscript: function(e4, before) {
              transcript = { ...transcript, root: e4.root, task: e4.task, title: e4.value?.title || transcript?.title, value: e4.value || transcript?.value, ref: e4.ref || transcript?.ref, id: "transcript-" + ++requests, loading: true, reason: null };
              const r4 = transcript;
              r4.timer = ui.defer(() => api.receive({ root: r4.root, id: r4.id, reason: "No transcript response within 65 seconds" }), 65e3);
              send({ a: "completed", root: r4.root, id: r4.id, task: r4.task, transcript: true, ...before == null ? {} : { before } });
            },
            transcript: function() {
              const t4 = transcript, p5 = t4?.page, c4 = t4?.value, [mark, tone, label] = c4 ? api.outcome(c4) : ["circle-question-mark", "", "Outcome unknown"];
              return h5("section", { class: "task-transcript-page", tabindex: -1, "aria-label": "Read-only task transcript" }, h5("div", { class: "popover-heading" }, h5("h2", null, t4?.title || "Task transcript"), button("arrow-left", "Back to tasks", { "data-a": "popup-close" })), h5("p", { class: "meta" }, icon(mark), h5("span", { class: tone }, label), c4?.close ? " \xB7 " + c4.close : "", c4?.result ? " \xB7 " + c4.result : ""), h5("div", { class: "row-actions" }, c4?.url && h5("a", { href: c4.url, "data-url": c4.url }, "Open issue"), c4?.pr && h5("a", { href: c4.pr, "data-url": c4.pr }, "Open PR")), h5("p", { class: "meta" }, "Read-only native transcript"), t4?.loading && h5("p", { role: "status" }, "Reading transcript\u2026"), t4?.reason && h5("p", { role: "status" }, "Transcript unavailable: " + t4.reason), p5 && h5("div", { class: "pin-transcript" }, p5.start > 0 && h5("button", { "data-a": "pin-transcript-earlier", disabled: t4.loading }, "Load earlier"), p5.items.map((i4, k5) => h5("p", { key: k5 }, h5("strong", null, i4.role + " \xB7 "), i4.text)), !p5.items.length && h5("p", { class: "meta" }, "No messages in this chunk.")));
            },
            list: function() {
              if (view.popup?.kind === "transcript") return api.transcript();
              if (api.current().id === "completed") return h5(Fragment, null, api.pins(), api.bar(), api.completed());
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
              return h5(Fragment, null, api.pins(), api.bar(), h5("header", { class: "filter-header tqp" }, h5("div", { class: "state-filters", dangerouslySetInnerHTML: { __html: blocks.counters(counts.counts, "", "Selected projects", partial, c4.states, counts.open) } }), rows.filter((t4) => t4.state === "held").length > 1 && h5("span", { class: "held-shortcut" }, app.startButton(rows))), errors.map((reason) => h5("p", { class: "meta", role: "status", key: reason }, reason)), app.answerNotices(), rows.length ? listing : h5("div", { class: "body meta" }, !known ? "Tasks unknown" : "No matching tasks", " ", h5("button", { "data-a": "filter-reset" }, "Reset")));
            },
            // MP14: Completed reads board history pages on demand through the tray (`taskq status --closed`); nothing is stored.
            since: function(period) {
              const days = { "7d": 7, "30d": 30 }[period];
              return days && Number.isFinite(view.S.now) ? view.S.now - days * 86400 : null;
            },
            request: function(e4) {
              e4.loading = true;
              e4.reason = null;
              e4.id = String(++requests);
              const id = e4.id;
              e4.timer = ui.defer(() => api.receive({ root: e4.root, id, reason: "No history response within 65 seconds" }), 65e3);
              send({ a: "completed", root: e4.root, id, page: e4.page + 1, ...e4.since == null ? {} : { since: e4.since } });
            },
            history: function() {
              const c4 = api.criteria(), key = JSON.stringify([c4.projects, c4.period]), prior = view.completed;
              if (api.current().id === "completed" && prior?.key !== key) {
                const since = api.since(c4.period);
                prior?.entries.forEach((e4) => ui.cancel(e4.timer));
                view.completed = { key, entries: api.unique(api.projects()).map((p5) => {
                  const old = prior?.entries.find((e4) => e4.identity === views.identity(p5));
                  return { root: p5.root, name: p5.name, kind: p5.board_kind, identity: views.identity(p5), page: 0, more: true, tasks: old?.tasks || [], shown: old?.shown || [], replace: true, since };
                }) };
                view.completed.entries.forEach(api.request);
              }
              const entries = view.completed?.entries || [];
              return { entries, rows: entries.flatMap((e4) => e4.tasks.map((t4) => ({ ...t4, project: e4.name, kind: e4.kind, ref: e4.identity + "#" + t4.n }))) };
            },
            receive: function(page) {
              if (transcript?.id === page.id && transcript.root === page.root) {
                ui.cancel(transcript.timer);
                transcript.loading = false;
                transcript.reason = page.reason || page.transcript?.reason;
                transcript.page = page.transcript?.items ? page.transcript : transcript.page;
                present();
                return;
              }
              const pin = [...pinReads.values()].find((e5) => e5.id === page.id && e5.root === page.root);
              if (pin) {
                if (!pin.loading) return;
                ui.cancel(pin.timer);
                pin.loading = false;
                pin.reason = page.reason;
                pin.value = !page.reason && page.tasks?.[0]?.n === pin.task ? page.tasks[0] : null;
                if (!pin.value && !pin.reason) pin.reason = "Board response omitted the task";
                present();
                return;
              }
              const e4 = view.completed?.entries.find((e5) => e5.id === page.id && e5.root === page.root);
              if (!e4?.loading) return;
              ui.cancel(e4.timer);
              e4.loading = false;
              if (page.reason) e4.reason = page.reason;
              if (!page.reason || page.source === "service" && page.tasks?.length && !e4.tasks.length) {
                if (e4.replace) {
                  e4.tasks = [];
                  e4.replace = false;
                }
                e4.page = page.page || e4.page + 1;
                e4.more = page.more === true;
                e4.tasks.push(...(page.tasks || []).filter((t4) => !e4.tasks.some((x5) => x5.n === t4.n)));
              }
              present();
            },
            outcome: function(t4) {
              return t4.outcome === "success" && typeof t4.merged === "string" && t4.merged ? ["git-merge", "review", "Merged"] : outcomes[t4.outcome] || outcomes.unknown;
            },
            completed: function() {
              const c4 = api.criteria(), { entries } = api.history(), pinned = new Set(api.model().pins.map((p5) => p5.ref));
              const shown = entries.flatMap((e4) => {
                if (!e4.replace) e4.shown = e4.tasks.filter((t4) => views.matches(t4, c4.query) && (!c4.outcomes.length || c4.outcomes.includes(t4.outcome in outcomes ? t4.outcome : "unknown")));
                return e4.shown.map((t4) => ({ ...t4, project: e4.name, kind: e4.kind, ref: e4.identity + "#" + t4.n }));
              }).filter((t4) => !pinned.has(t4.ref)).sort((a4, b5) => (b5.closed ?? 0) - (a4.closed ?? 0) || b5.n - a4.n);
              const loading = entries.some((e4) => e4.loading), more = entries.filter((e4) => e4.more && !e4.loading && !e4.reason);
              return h5(
                Fragment,
                null,
                shown.map((t4) => {
                  const [mark, tone, label] = api.outcome(t4);
                  return h5(
                    "div",
                    { class: "task-row completed-row", key: t4.ref, "data-completed": t4.ref },
                    h5("span", { class: tone, title: label, "aria-label": label }, icon(mark)),
                    h5("a", { class: "detail-title", href: t4.url, "data-url": t4.url }, t4.title),
                    h5("div", { class: "meta" }, icon(t4.kind), " ", t4.project + " #" + t4.n, " \xB7 ", label, " \xB7 ", Number.isFinite(t4.closed) ? new Date(t4.closed * 1e3).toISOString().slice(0, 10) : "closed date unknown", t4.result ? " \xB7 " + t4.result : "", t4.pr && h5(Fragment, null, " \xB7 ", h5("a", { href: t4.pr, "data-url": t4.pr }, icon("git-pull-request"), "PR"))),
                    h5("button", { "data-a": "completed-transcript", "data-ref": t4.ref }, "Open transcript")
                  );
                }),
                entries.map((e4) => h5(
                  "p",
                  { class: "meta", role: e4.reason ? "alert" : "status", key: "source" + e4.identity, "data-history-source": e4.identity },
                  e4.name + ": " + (e4.loading ? "Reading page " + (e4.page + 1) + "\u2026" : e4.reason ? "History unavailable \xB7 " + e4.reason : "Page " + e4.page + " loaded \xB7 " + e4.tasks.length + " rows" + (e4.more ? " \xB7 more available" : "")),
                  e4.replace && e4.shown.length > 0 ? " \xB7 Showing previous rows" : "",
                  e4.reason && h5("button", { "data-a": "completed-retry", "data-root": e4.root, "aria-label": "Retry " + e4.name + " history" }, "Retry")
                )),
                !loading && !shown.length && h5("div", { class: "body meta" }, entries.some((e4) => e4.reason) ? "Completed history incomplete" : entries.length ? "No completed tasks match" : "No projects selected"),
                more.length > 0 && h5("button", { class: "completed-more", "data-a": "completed-more" }, "Load more")
              );
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
                m5.views.map((v4) => h5("div", { class: "saved-view", key: v4.id }, h5("button", { "data-a": "view-select", "data-id": v4.id, "aria-pressed": String(v4.id === m5.selected) }, v4.name, views.dirty(v4) ? " *" : ""), button(v4.pinned ? "pin" : "pin-off", (v4.pinned ? "Remove shortcut for " : "Add shortcut for ") + v4.name, { "data-a": "view-pin", "data-id": v4.id, "aria-pressed": String(v4.pinned) }), !views.builtin(v4.id) && button("trash-2", "Delete view " + v4.name, { "data-a": "view-delete", "data-id": v4.id }))),
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
              if (!p5 || ["task", "transcript"].includes(p5.kind)) return null;
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
              if (view.popup && !["task", "transcript"].includes(view.popup.kind)) popover.place(menu.querySelector("#taskq-popover"), view.popup.anchor);
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
              if (d5.a === "pin-transcript" || d5.a === "completed-transcript") {
                const returnScroll = menu.querySelector(".content")?.scrollTop || 0, entry = d5.a === "pin-transcript" ? pinReads.get(d5.ref) : api.history().entries.flatMap((e5) => e5.tasks.map((t4) => ({ ref: e5.identity + "#" + t4.n, root: e5.root, task: t4.n, value: t4 }))).find((e5) => e5.ref === d5.ref);
                if (entry?.value?.state === "closed") {
                  transcript = { returnScroll };
                  api.readTranscript(entry);
                  api.unpin(d5.ref);
                  api.open("transcript", b5, d5.ref);
                  const content = menu.querySelector(".content");
                  if (content) content.scrollTop = 0;
                }
                return true;
              }
              if (d5.a === "pin-transcript-earlier") {
                if (transcript?.page?.start > 0 && !transcript.loading) {
                  api.readTranscript(transcript, transcript.page.start);
                  present();
                }
                return true;
              }
              if (d5.a === "view-select") {
                if (!owned) api.current().scroll = menu.querySelector(".content")?.scrollTop || 0;
                m5.selected = d5.id;
                view.popup = null;
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
              if (d5.a === "completed-retry") {
                const entry = view.completed?.entries.find((e5) => e5.root === d5.root && e5.reason && !e5.loading);
                if (entry) {
                  api.request(entry);
                  present();
                }
                return true;
              }
              if (d5.a === "completed-more") {
                view.completed?.entries.filter((e5) => e5.more && !e5.loading && !e5.reason).forEach(api.request);
                present();
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
                if (view.popup && !["task", "transcript"].includes(view.popup.kind) && !e4.target.closest('#taskq-popover,#taskq-question-details,[data-a="task-detail"],[data-a="view-editor"],[data-a="view-list"]')) api.close(false);
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

  // src/relative-time.js
  var require_relative_time = __commonJS({
    "src/relative-time.js"(exports, module) {
      "use strict";
      var { h: h5 } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      var mounted = /* @__PURE__ */ new WeakSet();
      var relative = new Intl.RelativeTimeFormat("en", { numeric: "always", style: "short" });
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
          if (age < 60) return delta > 0 ? "in a moment" : "just now";
          const [unit, size] = age < 3600 ? ["minute", 60] : age < 86400 ? ["hour", 3600] : ["day", 86400];
          return relative.format(Math.sign(delta) * Math.floor(age / size), unit);
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
          return h5(ms === null ? "span" : "time", { class: "relative-time" + (ms === null ? " time-unknown" : ""), ...ms === null ? {} : { dateTime: new Date(ms).toISOString(), "data-time-ms": ms, "data-time-source": raw, "data-time-context": context }, title: full, "aria-label": full, tabIndex: focus ? 0 : void 0 }, ms === null ? "Time unknown" : time.label(ms));
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
            target = node?.closest(".relative-time") || node?.closest("button,summary")?.querySelector(".relative-time");
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
          return replyFiles.problem(files) || (text.trim() || files?.length ? null : "Type an answer");
        },
        status: function(row, local, cap) {
          const q4 = row.question;
          if (local.op && local.receipt && (HELD.includes(local.receipt.state) || local.op.revision === row.revision || !q4)) return local.receipt.state;
          if (!manageable(row, cap) && (!q4 || !q4.parsed || !cap || !cap.answer || !["ask", "review"].includes(row.state))) return "read-only";
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
              return start({ choice: c4.id });
            }
            case "submit":
              if (!cap || !cap.answer || !LIVE.includes(st) || textProblem(draftQuestion(row, cap), l5.draft, l5.files)) return { local, send: null };
              return start(fallback(row, cap) ? { action: "requeue", text: l5.draft.trim() } : { text: l5.draft.trim() }, st === "changed" && !fallback(row, cap) ? l5.bound : row.revision);
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
            h5 += q4.parsed ? lines(q4) + images(q4) : '<pre class="qac-text">' + esc(q4.text || row.card || "No question text supplied") + "</pre>";
            if (own) h5 += q4.parsed ? '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="2" aria-label="Your answer" placeholder="What you did"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(q4, local.draft, local.files) ? "" : " disabled") + ">Send</button>" + hint(id, live && textProblem(q4, local.draft, local.files)) + "</form>" : '<form class="qac-text-form" data-qac="submit"><textarea data-qac="input" rows="1" aria-label="Requeue reason" placeholder="Reason to requeue"' + (live ? "" : " readonly") + ">" + esc(local.draft) + '</textarea><button type="submit"' + (live && !textProblem(REQUEUE, local.draft, local.files) ? "" : " disabled") + ">Requeue</button>" + hint(id, live && textProblem(REQUEUE, local.draft, local.files)) + "</form>";
            h5 += '<div class="qac-choices">' + (q4.parsed && own ? '<button type="button" data-qac="requeue"' + (live && !textProblem(REQUEUE, local.draft, local.files) ? "" : " disabled") + ">Requeue</button>" : "") + '<button type="button" data-qac="later"' + (live ? "" : " disabled") + '>Later</button><button type="button" data-qac="close"' + (live ? "" : " disabled") + ">Close without publishing</button></div>";
            h5 += receipt(st, local.receipt && local.receipt.reason, row.url, id, local.receipt && local.receipt.next, !!(local.op && local.op.action), what, local.receipt?.note);
            h5 += retry(st, local.receipt && local.receipt.reason);
            return h5 + "</section>";
          }
          if (!q4.parsed) return h5 + '<pre class="qac-text">' + esc(q4.text) + "</pre>" + receipt(st, st === "read-only" ? "This card has no answer form here" : local.receipt && local.receipt.reason, row.url, id, null, false, what) + retry(st, local.receipt && local.receipt.reason) + "</section>";
          h5 += lines(q4) + images(q4);
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
              if (cards.has(k5) && ["submit", "requeue"].includes(type)) apply(k5, { type });
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
        create: function(view, { menu, document: document2, send, draw, defer = ui.defer, cancel = ui.cancel, composer = false, changed, steer }) {
          const answerRequests = /* @__PURE__ */ new Map(), answerCards = /* @__PURE__ */ new Map(), answerFeedback = /* @__PURE__ */ new Map(), saved = {}, steers = {};
          const api = execution.module("workspace.answers.instance", {
            project: function(board, root = view.answer?.board === board ? view.answer.root : null) {
              return view.S?.projects.find((p5) => p5.issues === board && (!root || p5.root === root));
            },
            answerCapability: function(board) {
              const p5 = api.project(board);
              return { manage: true, files: true, answer: !!p5 && p5.alive && p5.complete === true, reason: "Project status is unavailable or stale; check the board", composer };
            },
            // #1662 A3: the workspace's shared composer reads and drives the mounted card; the card keeps draft, op and receipt
            answerCompose: function(board, row) {
              return component().compose(row, saved[board + "#" + row.n] || component().fresh(row), answerCapability(board));
            },
            answerInput: function(board, n3, text, caret) {
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
            answerHistory: function(t4) {
              return (t4.history || []).map(({ question: q4, answered: a4 }, i4) => {
                const row = { n: t4.n, title: t4.title, url: t4.url, state: "closed", revision: a4?.revision || 0, question: { choices: [], evidence: [], ...q4 } }, key = a4?.revision ?? i4;
                return [
                  h5("section", { key: "asked:" + key, class: "thread-card asked", "data-reading-key": "question:" + t4.n + ":" + key, "aria-label": "Question " + (i4 + 1) + " of #" + t4.n }, time.stamp(null, "Recorded question", "Question timestamp unavailable in the history adapter"), h5("div", { dangerouslySetInnerHTML: { __html: component().render("history:" + t4.n + ":" + key, row, component().fresh(row), { answer: false, manage: false, composer: true, reason: "Recorded question" }) } })),
                  h5("p", { key: "answered:" + key, "data-reading-key": "answer:" + t4.n + ":" + key, class: "body answered", role: "status" }, won({ answered: a4 || {} }), time.stamp(null, "Recorded answer", "Answer timestamp unavailable in the history adapter"))
                ];
              });
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
                  save(board, t4, { ...saved[key] || component().fresh(t4), draft: e4.currentTarget.value, steerAttempt: t4.attempt });
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
              for (const slot of menu.querySelectorAll("[data-answer]")) {
                const t4 = unique(view.S.projects.filter((p6) => !view.answer?.root || p6.root === view.answer.root).flatMap(tasks)).find((t5) => t5.key === slot.dataset.answer);
                if (!t4) continue;
                let card = answerCards.get(t4.key);
                if (!card) {
                  const el = document2.createElement("div");
                  card = { el, ui: component().mount(el, { capability: answerCapability, send: answerSend, load, save }) };
                  answerCards.set(t4.key, card);
                }
                if (card.el.parentNode !== slot) slot.append(card.el);
                const p5 = api.project(t4.key.slice(0, t4.key.lastIndexOf("/")));
                card.ui.update([{ board: p5.issues, row: t4 }]);
                for (const d5 of slot.querySelectorAll("details")) {
                  const key = "answer:" + t4.key + ":" + d5.className;
                  if (d5.dataset.viewKey !== key) d5.dataset.viewKey = key;
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
                const board = key.slice(0, key.lastIndexOf("#")), p5 = api.project(board, local.projectRoot);
                if (local.op && p5 && saved[key].receipt) answerFeedback.set(p5.root + "#" + local.op.id, { board, op: local.op, about: local.about, ...saved[key].receipt });
              }
            },
            retry: function(key) {
              const r4 = answerFeedback.get(key);
              if (r4) ui.effect("workspace.answer.reconcile", () => answerSend(r4.board, r4.op), () => {
              });
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
          return { dashboard: null, popup: null, manualSize: null, agentqRuntime: null, agentqError: null, aiRoot: null, addConfirm: false, resourceNotice: null, persistenceError: null, toast: null, receivedAt: 0, S: snapshot, page: "home", taskFilter: null, working: null, workCard: null, ask: null, noted: null, pending: {}, hostCommand: null, recovery: {}, confirming: null, updating: null, draft: null, disclosures: {}, pressedAt: null, stale: false, resizing: null };
        },
        create: function(view, ports) {
          const api = execution.module("tray.create", {
            operationLauncher: function(operation, root, label) {
              return h5("button", { class: "project", "data-a": "workspace", "data-page": "operations", "data-operation": operation, "data-root": root || null }, label);
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
              const names = projects2.map((p5) => p5.name).join(", ") || "no registered projects";
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
                rows,
                api.lifecycleWaits()
              );
            },
            recoveryPanel: function() {
              if (!ports.workspace) return api.operationLauncher("recovery", null, "Review recovery\u2026");
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
            mode: function(p5) {
              return p5.mode;
            },
            counter: function(v4, kind, label) {
              return h5("span", { class: "counter " + (v4 > 0 ? kind + " filled" : "zero"), "aria-label": label + ": " + v4, title: label }, v4);
            },
            known: function() {
              return !view.S.display.all.partial.length;
            },
            // O1: identity anchors details/actions; title remains a full-width issue link.
            taskRow: function(t4, global) {
              const p5 = view.S.projects.find((p6) => p6.root === t4.root);
              return h5("div", { class: "task-row", "data-card": t4.key, tabindex: -1, key: t4.key }, raw(TaskqProjectBlocks.task(blockProject(p5), t4, global, true)), api.syncLine(t4.sync), dashboard?.attention(t4));
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
              return h5("button", { "data-a": "start", "data-keys": held.map((t4) => t4.key).join(" "), title: label, "aria-label": label }, icon("play"), h5("span", null, label));
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
              return { run: "Configured intake: " + p5.focus.join(" "), paused: "Paused: running tasks finish, no new ones", watch: "Observation only: focus off" }[mode(p5)];
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
                api.disclosure("activity", ["Service ", h5("span", { class: "meta" }, p5.name + (host ? "" : " \xB7 " + (p5.age == null ? "status unknown" : p5.age_text + " old")))]),
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
              const reduced = window2.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;
              return f4(
                api.heading("Text size \xB7 " + view.S.font + " px", "type"),
                h5("div", { class: "body meta" }, "Theme follows system appearance."),
                h5("div", { class: "options" }, [14, 16, 18, 20].map((n3) => h5("button", { "data-set": "font", "data-v": n3, "aria-pressed": String(n3 === view.S.font), style: "font-size:" + n3 + "px" }, n3))),
                api.heading("Tray icon", "panels-top-left"),
                views(),
                api.heading("Window transparency", "sliders-horizontal"),
                h5("div", { class: "options" }, [["whole", "Whole window"], ["background", "Background only"]].map(([v4, label]) => h5("button", { "data-set": "opacity_mode", "data-v": v4, "aria-pressed": String((view.S.opacity_mode || "whole") === v4) }, label))),
                h5("div", { class: "body opacity-setting" }, h5("label", { for: "menu-opacity" }, "Menu opacity \xB7 " + (view.S.opacity ?? 100) + "%"), h5("button", { "data-set": "opacity", "data-v": 100, disabled: view.S.opacity_mode === "background", "aria-pressed": String((view.S.opacity ?? 100) === 100) }, "Off"), h5("input", { id: "menu-opacity", type: "range", min: 60, max: 100, step: 1, value: view.S.opacity ?? 100, disabled: view.S.opacity_mode === "background", "data-opacity": "opacity", "aria-valuetext": (view.S.opacity ?? 100) === 100 ? "Off \xB7 100% opaque" : view.S.opacity + "% opaque" })),
                h5("div", { class: "body meta" }, view.S.opacity_mode === "background" ? "Select Whole window to adjust menu opacity." : "Whole window: 60\u2013100% opaque, including text and controls."),
                h5("div", { class: "body opacity-setting" }, h5("label", { for: "background-opacity" }, "Background opacity \xB7 " + (view.S.background_opacity ?? 100) + "%"), h5("input", { id: "background-opacity", type: "range", min: 0, max: 100, step: 1, value: view.S.background_opacity ?? 100, disabled: view.S.opacity_mode !== "background", "data-opacity": "background_opacity", "aria-valuetext": (view.S.background_opacity ?? 100) + "% opaque" })),
                h5("div", { class: "body meta" }, view.S.opacity_mode === "background" ? "The task filter background stays opaque. Other fills fade; text and controls stay opaque. At 0%, the desktop shows through." : "Select Background only to adjust fills while keeping text and controls opaque."),
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
                api.disclosure(r4.runtime === "claude" ? "claude" : "openai", [runtimeName(r4.runtime) + " " + (r4.version || "version unknown") + " \xB7 ", h5("span", { class: !A4.checking && signAlert.includes(r4.state) ? "alert" : "" }, label(r4)), h5("span", { class: "meta" }, " \xB7 " + (r4.account || "account unknown"))]),
                h5("div", { class: "body meta" }, lines([A4.checking && "Last result: " + prior(r4), r4.method && "Method: " + r4.method, r4.expires ? "Expires " + when(r4.expires) : "Expiry unknown", r4.refreshed && "Last refresh " + r4.refreshed + "; not an expiry", r4.reason && "Reason: " + r4.reason, "Runtime client " + (r4.version || "version unknown") + " \xB7 " + checked].filter(Boolean))),
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
                    ["claude", "codex", "codex-app"].map((rt) => h5("div", { class: "body meta" }, runtimeName(rt) + " \xB7 unknown: " + n3 + " heartbeat publishes no client version, sign-in, account or access")),
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
              const local2 = view.S.projects.filter((p5) => p5.host === view.S.host), missing = local2.filter((p5) => !Array.isArray(p5.access)).map((p5) => p5.name);
              return [
                local2.filter((p5) => Array.isArray(p5.access)).map((p5) => h5(
                  "details",
                  { class: "group access", "data-view-key": "access:" + p5.root },
                  api.disclosure("shield", ["Access ", h5("span", { class: "meta" }, p5.name + " \xB7 " + (p5.access.map((a4) => runtimeName(a4.runtime) + " " + (view.S.agents?.rows?.find((r4) => r4.runtime === a4.runtime)?.version || "version unknown") + " " + (a4.refused ? "refused" : a4.effective || "unknown")).join(" \xB7 ") || "no runtime"))]),
                  p5.access.map((a4) => h5("div", { class: "body meta" }, runtimeName(a4.runtime) + ": " + (a4.refused ? "refused: " + a4.refused : "requested " + (a4.requested || "unknown") + " \xB7 saved grant " + (a4.grant || "none") + " \xB7 applied " + (a4.effective || "unknown") + (a4.enforcement ? " \xB7 " + a4.enforcement : "")))),
                  h5("div", { class: "body meta" }, "Applied at the last service pass \xB7 " + (p5.age == null ? "age unknown" : p5.age_text + " old"))
                )),
                missing.length > 0 && h5("div", { class: "body meta" }, "Access unknown for " + missing.join(", ") + ": no access in status.json"),
                !local2.length && h5("div", { class: "body meta" }, "Access unknown: no project on this host")
              ];
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
              const parts = [api.agentq(), setup(), ...view.S.projects.flatMap((p5) => [problem(p5), incident(p5, true), incidentNotice(p5)]), globalGroup("ask", all), globalGroup("failed", all), answerNotices()].filter(Boolean);
              return parts.length ? f4(parts) : "";
            },
            projects: function() {
              return f4(api.heading("Registered projects", "folder"), raw(TaskqProjectBlocks.aggregate(view.S.display.all, [])), view.S.projects.length ? view.S.projects.map((p5) => [raw(TaskqProjectBlocks.summary(blockProject(p5))), h5("div", { class: "body meta" }, modeText(p5)), h5("details", { class: "request", "data-view-key": "request:" + p5.root }, api.disclosure("message-circle-question", "Request"), h5("div", { class: "body meta" }, "You asked: " + (p5.request || "not recorded")))]) : h5("div", { class: "body meta" }, "No projects yet"), addProject());
            },
            controls: function() {
              const run = view.S.projects.filter((p5) => mode(p5) === "run"), held = view.S.projects.filter((p5) => mode(p5) === "paused");
              return f4(workEntry(), held.length > 0 && run.length > 0 && h5("div", { class: "row meta" }, "Paused " + held.length + " of " + (held.length + run.length) + ": " + held.map((p5) => p5.name).join(", ")), bulk(run, held));
            },
            runningAgents: function(compact = false) {
              const hosts = /* @__PURE__ */ new Map(), all = view.S.display.all.counts;
              let partial = false;
              const seen = /* @__PURE__ */ new Map();
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
                const stale = fresh && read ? null : (!read ? "missing host/task data" : !p5.complete ? "partial board snapshot" : elapsed === null ? "missing task observation" : "stale task observation") + " (" + (p5.name || p5.root) + ")";
                partial ||= !fresh || !Array.isArray(p5.hosts) || !Array.isArray(p5.tasks);
                for (const h6 of Array.isArray(p5.hosts) ? p5.hosts : []) {
                  if (!h6 || typeof h6.name !== "string" || !h6.name) {
                    partial = true;
                    continue;
                  }
                  const age = Number.isFinite(h6.age) ? h6.age : null, host = add(h6.name, stale);
                  host.beat = true;
                  host.live ||= h6.live === true && !stale;
                  if (age !== null && (host.age === null || age < host.age)) Object.assign(host, { age, age_text: h6.age_text });
                }
                for (const t4 of Array.isArray(p5.tasks) ? p5.tasks : []) {
                  if (t4.executions != null && !Array.isArray(t4.executions)) partial = true;
                  for (const x5 of Array.isArray(t4.executions) ? t4.executions : []) {
                    if (!x5 || typeof x5.host !== "string") {
                      partial = true;
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
                const t4 = /* @__PURE__ */ new Map([["Claude", 0], ["Codex", 0]]);
                for (const [rt, c4] of host.counts) t4.set(family(rt), (t4.get(family(rt)) || 0) + c4.worker + c4.reviewer);
                return t4;
              };
              const known2 = (host) => host.live && !host.reasons.has("conflicting execution observations");
              const sum = (counts) => [...counts.values()].reduce((n3, x5) => n3 + x5, 0);
              const value = (n3, known3, partial2) => !known3 ? "?" : partial2 ? n3 + "+?" : String(n3);
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
                    h5("span", { class: "host-runtime-split" }, ["Codex", "Claude", ...[...t4.keys()].filter((rt) => !["Codex", "Claude"].includes(rt))].map((rt) => runtime(rt, t4.get(rt) || 0, !unknown, partial2))),
                    h5("span", { class: "host-row-total", "aria-label": "Total running agents: " + total }, h5("span", { class: "host-number" }, total)),
                    (unknown || partial2 || warnings.length > 0) && h5("span", { class: "host-observation" }, unknown ? "Status unknown" + age : partial2 ? "Partial observation" + age : "", warnings.length > 0 && h5("span", { class: "host-warning" }, "Sign-in: " + warnings.join(" \xB7 ")))
                  ),
                  h5("div", { class: "body meta" }, "Heartbeat" + age),
                  unknown ? h5("div", { class: "body meta" }, "Unknown: " + (why || "host observation missing or stale")) : why && h5("div", { class: "body meta" }, "Partial: " + why),
                  !unknown && (runtimes.length ? h5("div", { class: "body" }, runtimes.map(([rt, c4]) => h5("div", null, rt + ": " + c4.worker + " worker" + (c4.worker === 1 ? "" : "s") + " \xB7 " + c4.reviewer + " reviewer" + (c4.reviewer === 1 ? "" : "s")))) : h5("div", { class: "body meta" }, "No running agents")),
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
                return [host.name, ...!known2(host) ? ["runs unknown", ...age ? [age + " old"] : [], ...host.reasons.size ? [[...host.reasons].join("; ")] : []] : [...busy.length ? busy.map(([k5, n3]) => k5 + " " + n3) : ["idle"], ...host.reasons.size ? ["partial: " + [...host.reasons].join("; ")] : []], ...warn(host)].join(" \xB7 ");
              };
              const overall = /* @__PURE__ */ new Map([["Codex", 0], ["Claude", 0]]);
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
              if (compact) return { label: strip.map(text).join("; ") || "Host unknown", meta: "", strip: h5("span", { class: "host-overview" }, h5("span", { class: "host-strip" }, strip.length ? strip.map(chip) : h5("span", { class: "host-chip unknown" }, "Hosts ?")), h5("span", { class: "host-runtime-totals", "aria-label": "All observed hosts \xB7 running agents" }, [...overall].map(([rt, n3]) => runtime(rt, n3, anyKnown, uncertain)))) };
              return h5(
                "section",
                { class: "group running-agents", style: { "--host-number-width": numberWidth }, "aria-label": "Running agents" },
                h5("div", { class: "host-grid-head", "aria-label": "Host, Codex, Claude, Total running agents" }, h5("span", null, "Host"), ["Codex", "Claude", "Total"].map((rt) => h5("span", { title: rt, "aria-label": rt, role: "img" }, icon(mark(rt))))),
                out,
                h5("div", { class: "body meta" }, "Registered project boards \xB7 " + (partial || all.some((c4) => c4.n === null) ? "Tasks unknown" : "Tasks: " + all.filter((c4) => ["doing", "review"].includes(c4.state)).map((c4) => c4.name + " " + c4.n).join(" \xB7 "))),
                h5("details", { "data-view-key": "run-definitions" }, api.disclosure("info", "Count definitions"), h5("div", { class: "body meta" }, "Running processes, occupied slots, reservations and board tasks are counted apart. Reservations include pending or unconfirmed work; occupied slots can include other projects."))
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
                  return f4(h5("p", { class: "meta" }, "Delivery is project policy, separate from Access. Exact-candidate approval and publication remain on the board."), view.S.projects.map((p5) => h5("details", { class: "group", "data-view-key": "delivery:" + p5.root }, api.disclosure("git-pull-request", [p5.name + " \xB7 ", h5("span", { class: "meta" }, p5.delivery?.preset || "policy unavailable")]), h5("div", { class: "body meta" }, "Revision " + (p5.delivery?.revision ?? "unknown")), p5.issues && h5("a", { class: "project", href: p5.issues, "data-url": p5.issues }, "Open Board"))));
                case "appearance":
                  return appearance();
                case "cadence":
                  return history2();
                case "notifications":
                  return notificationPanel.body();
                case "notification-settings":
                  return notifications();
                case "agents":
                  return agents();
                case "work":
                  return f4(api.heading("Host admission", "focus"), view.S.projects.some((p5) => p5.resources) ? api.workPage() : api.operationLauncher("admission", null, "Review admission in workspace\u2026"));
                case "stop":
                  return api.stopPanel();
                case "recovery":
                  return f4(api.heading("Recovery", "rotate-ccw"), api.recoveryPanel());
                case "services":
                  return f4(api.heading(view.S.host || "This host", "activity"), api.hostService(), api.operationLauncher("service", null, "Host maintenance in workspace\u2026"), view.S.projects.map((p5) => [problem(p5), incidentNotice(p5), service(p5, false)]));
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
              r4.resolve(receipt);
              defer(fit, 0);
            },
            capacity: function(p5) {
              if (!ports.workspace) return p5.resources ? api.operationLauncher("resources", p5.root, "Review capacity\u2026") : "";
              if (!p5.resources) return "";
              return h5("details", { class: "group", "data-view-key": "capacity:" + p5.root }, h5("summary", null, icon("sliders-horizontal"), "Capacity ", h5("span", { class: "meta" }, (p5.cap_host || "this host") + " \xB7 " + require_resource_controls().headline(resourceGroup(p5), view.S.now))), h5("div", { "data-resources": p5.root }));
            },
            workEntry: function(root) {
              return view.S.projects.some((p5) => p5.resources) ? h5("button", { class: "project", "data-a": "work", "data-root": root || null }, icon("focus"), "Work mode\u2026") : "";
            },
            workPage: function(root) {
              if (!ports.workspace) return api.operationLauncher("resources", Object.keys(view.working || {})[0] || null, "Review host limits in workspace\u2026");
              const ps = view.S.projects.filter((p6) => p6.resources), p5 = ps.find((p6) => p6.root === root || view.working?.[p6.root]) || ps[0], run = view.S.projects.filter((p6) => mode(p6) === "run"), held = view.S.projects.filter((p6) => mode(p6) === "paused");
              return f4(h5("div", { class: "body", role: "status" }, "Admission \xB7 " + api.admissionText()), bulk(run, held), api.resourceMeters(p5), h5("details", { "data-view-key": "admission-reasons" }, h5("summary", null, "Admission details"), view.S.projects.map((p6) => h5("div", { class: "body meta" }, p6.name + ": " + (p6.capacity?.reasons?.map((r4) => r4.text || r4.code).join(" \xB7 ") || "No reported reasons")))), p5?.resources.targets?.some((t4) => t4.supported?.some((s4) => s4.policy_schema === "taskq.host-capacity/1")) ? h5("div", { "data-resources": p5.root }) : h5("div", { "data-work": "" }), api.accessFacts());
            },
            admissionText: function(ps = view.S.projects) {
              if (!ps.length) return "No projects";
              const rows = ps.map((p5) => {
                const c4 = p5.capacity, reasons = Array.isArray(c4?.reasons) ? c4.reasons : null;
                const unknown = p5.alive !== true || !reasons || !["open", "off", "held"].includes(c4?.gate) || c4.gate === "held" && !reasons.length;
                return { name: p5.name, state: unknown ? "unknown" : reasons.length ? "blocked" : "clear", text: unknown ? "observation unknown" : reasons.length ? [...new Set(reasons.map((r4) => r4.code || "unknown"))].join(", ") : "no reported blockers" };
              });
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
              const N2 = view.S.notify, st = N2.status, line = { allowed: "Allowed by the OS", blocked: "Blocked by the OS: nothing is posted", "not asked": "Not asked yet: the first banner setting asks the OS", asking: "Asked: answer the OS prompt", "request failed": "OS permission request failed: " + N2.failure, unknown: "OS permission unknown", unsupported: "Not available on this host" }[st] || st;
              return f4(
                api.heading("OS permission", "shield"),
                h5("div", { class: "body" + (st === "blocked" || st === "request failed" ? " alert" : " meta"), role: "status" }, line),
                st === "blocked" && h5("button", { class: "project", "data-a": "notify-os" }, "Open OS setting \u2197"),
                api.heading("Per event on this host", "bell"),
                view.S.display.notify.map((x5) => [x5.id, x5.name]).map(([k5, l5]) => [api.heading(l5, { ask: "message-circle-question", close: "circle-check", failed: "circle-x", gone: "circle-alert" }[k5] || "bell"), h5("div", { class: "options" }, modes.map(([v4, m5]) => h5("button", { "data-a": "notify", "data-kind": k5, "data-v": v4, "aria-pressed": String(N2.host[k5] === v4) }, m5)), h5("button", { "data-a": "notify-test", "data-kind": k5, disabled: st !== "allowed" }, "Send test"))])
              );
            },
            notifyGroup: function(p5) {
              const N2 = view.S.notify.projects[p5.root];
              if (!N2) return "";
              const name = (v4) => modes.find((m5) => m5[0] === v4)[1];
              return h5("details", { class: "group", "data-view-key": "notifications:" + p5.root }, h5("summary", { class: "line" }, icon("bell"), "Notifications ", h5("span", { class: "meta" }, view.S.display.notify.map((x5) => [x5.id, x5.name]).map(([k5]) => name(N2[k5][1])).join(" \xB7 "))), view.S.display.notify.map((x5) => [x5.id, x5.name]).map(([k5, l5]) => [h5("div", { class: "body" }, l5 + " ", h5("span", { class: "meta" }, "\xB7 " + name(N2[k5][1]))), h5("div", { class: "options" }, [["inherit", "Inherit"], ...modes].map(([v4, m5]) => h5("button", { "data-a": "notify", "data-kind": k5, "data-root": p5.root, "data-v": v4, "aria-pressed": String((N2[k5][0] || "inherit") === v4) }, m5)))]));
            },
            openCard: function(o4) {
              const p5 = view.S && view.S.projects.find((p6) => p6.root === o4.root);
              if (!p5) return;
              if (view.page !== p5.root || view.working || view.taskFilter) navigate2(p5.root);
              const key = p5.issues + "/" + o4.n;
              const find = () => o4.n != null && menu.querySelector('[data-card="' + CSS2.escape(key) + '"]');
              if (o4.n != null && !find()) {
                view.noted = { root: p5.root, n: o4.n, key, url: key, title: o4.title || "", state: o4.kind === "close" ? "closed" : o4.kind, state_name: [...view.S.display.states, ...view.S.display.notify].find((x5) => x5.id === o4.kind)?.name };
                present(true);
              }
              const el = find();
              if (el) {
                el.scrollIntoView({ block: "center" });
                (el.querySelector("textarea,button,a") || el).focus({ preventScroll: true });
              }
            },
            notedCard: function(p5) {
              return view.noted && view.noted.root === p5.root ? h5("div", { class: "group" }, h5("div", { class: "row meta" }, h5("span", null, "Notification")), h5("div", { "data-card": view.noted.key, tabindex: "-1" }, raw(TaskqProjectBlocks.task(blockProject(p5), view.noted)))) : "";
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
                notedCard(p5),
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
              const S3 = view.S, ps = S3.projects, A4 = S3.agents, line = api.runningAgents(true), st = S3.notify?.status, svc = S3.service?.state, up = ps.find((p5) => p5.update && p5.update !== p5.version);
              const growth = { auto: "Auto", top: "Down", bottom: "Up", fixed: "Fixed" }[S3.size_anchor || "auto"] || "Auto";
              const widget = (target, mark, label, meta) => h5("button", { class: "project settings-widget" + (["notification-settings", "cadence", "projects", "delivery", "recovery"].includes(target["data-route"]) || target["data-operation"] === "stop" ? " short-value" : ""), ...target }, mark, h5("span", null, label), h5("span", { class: "meta" }, meta));
              const sections = [
                ["Display", [
                  widget({ "data-route": "appearance" }, icon("type"), "Appearance", S3.font + " px \xB7 " + (S3.view === "full" ? "Summary" : "Icon") + " \xB7 growth " + growth),
                  widget({ "data-route": "notification-settings" }, icon("bell"), "Notifications", { allowed: "OS allowed", blocked: "OS blocked", "not asked": "OS not asked", asking: "OS asking", "request failed": "OS request failed", unsupported: "OS unsupported" }[st] || "OS permission unknown"),
                  widget({ "data-route": "cadence" }, icon("chart-no-axes-column"), "Activity history", S3.display?.periods?.find((x5) => x5.id === S3.period)?.name || "period unknown")
                ]],
                ["Host and projects", [
                  widget({ "data-route": "hosts" }, icon("network"), "Hosts and resources", line.label),
                  widget({ "data-route": "work" }, icon("focus"), "Work mode", api.admissionText() + (ps.some((p5) => p5.resources) ? "" : " \xB7 capacity unavailable")),
                  widget({ "data-route": "agents" }, icon("bot"), "Agents and access", A4?.checking ? "checking sign-in" : A4 ? A4.rows.map((r4) => runtimeName(r4.runtime) + " " + (r4.state === "signed-in" && !A4.fresh ? "signed in at capture" : (signState[r4.state] || "Unknown").toLowerCase())).join(" \xB7 ") || "no runtime" : "sign-in unknown"),
                  widget({ "data-route": "projects" }, icon("folder"), "Projects", ps.length + " registered"),
                  widget({ "data-route": "delivery" }, icon("git-pull-request"), "Delivery", grouped(ps.map((p5) => p5.delivery?.preset || "unavailable")) || "no projects"),
                  widget({ "data-route": "services" }, icon("activity"), "Services and support", "host service " + (svc === "running" ? "running" : svc === "exited" ? "exited" : "unknown")),
                  S3.stop_modes && widget({ "data-a": "workspace", "data-page": "operations", "data-operation": "stop" }, icon("octagon-x"), "Emergency stop", "Drain or Full stop"),
                  S3.stop_modes && widget({ "data-route": "recovery" }, icon("rotate-ccw"), "Recovery", S3.stop_hold ? "admission held" : "no hold"),
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
              const p5 = view.S.projects?.find((p6) => p6.root === view.page), title = view.working ? "Work mode" : p5 ? p5.name : routes[view.page] || "Unavailable";
              const crumbs = p5 ? h5("a", { href: "#", "data-route": "projects" }, "Projects") : settingsPages.includes(view.page) && view.page !== "settings" ? h5("a", { href: "#", "data-route": "settings" }, "Settings") : "";
              if (ports.settings) {
                const W = view.settingsChrome || {};
                return shell.bar({ title, crumbs, home: "Settings home", back: view.page !== "settings" || renderer.entries().length > 0, actions: [], windows: [button("always-on-top", "Always on top", { "data-a": "settings-top", "aria-pressed": String(!!W.top) }), button(W.maximized ? "minimize-2" : "maximize-2", W.maximized ? "Restore window" : "Enlarge window", { "data-a": "settings-enlarge", "aria-pressed": String(!!W.maximized) }), button("x", "Close settings", { "data-a": "hide" })] });
              }
              if (ports.views) return shell.bar({ title: "View \xB7 " + dashboard.current().name, actions: [], windows: [button("x", "Close view window", { "data-a": "hide" })] });
              const m5 = view.S.messages, count = m5?.complete === true && Number.isSafeInteger(m5.count) ? m5.count : null;
              const conversations = h5("button", { type: "button", class: "conversations-entry", "data-a": "workspace", "data-page": "conversations", "data-thread": count ? "unread" : "", title: "Conversations \xB7 " + (count === null ? "unknown" : count + " new"), "aria-label": "Conversations \xB7 " + (count === null ? "unknown" : count + " new") }, icon("messages-square"), count ? h5("span", { class: "badge", "aria-hidden": "true" }, count > 99 ? "99+" : String(count)) : "");
              const actions = [p5?.issues && h5("a", { href: p5.issues, "data-url": p5.issues, title: "Open Board", "aria-label": "Open " + p5.name + " Board" }, icon(p5.board_kind)), view.S.stop_modes && button("octagon-x", "Emergency stop", { "data-a": "stop-review" }), notificationPanel.bell(), conversations, button("settings", "Settings", { "data-route": "settings" })];
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
              const title = view.working ? "Work mode" : view.taskFilter ? "Tasks" : p5 ? p5.name : routes[view.page];
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
            receive: function(s4) {
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
              if (document2.activeElement && (document2.activeElement.dataset.draft || document2.activeElement.dataset.rscField || document2.activeElement.dataset.recoveryEvidence || document2.activeElement.id === "ai")) return;
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
                  send({ a: "workspace", open: { page: "operations", operation: api.operationTarget(d5), root: d5.root || null } });
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
                  viewState.flush();
                  send({ a: "workspace", open: { page: d5.page || "conversations", ...d5.thread !== void 0 ? { thread: d5.thread } : {}, operation: d5.operation || null, root: d5.root || null, board: d5.board || null, task: Number(d5.task) || null, revision: Number(d5.revision) || null, ...d5.fresh === "true" ? { fresh: true } : {} } });
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
                  for (const key of d5.keys.split(" ")) {
                    const t4 = unique(view.S.projects.flatMap(tasks)).find((t5) => t5.key === key);
                    if (t4 && t4.state === "held") ui.effect("task.start", () => answerSend(t4.key.slice(0, -String(t4.n).length - 1), { id: "tray.start." + t4.n + "." + t4.revision, task: t4.n, revision: t4.revision, action: "start" }), () => {
                    });
                  }
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
          });
          const { natural, gutter, fit, actionReceipt, actionNotices, focusStart, focusReceipt, focusReadback, remember, navigate: navigate2, back, routeButton, answerSlot, answerNotices, answerToast, tasks, blockProject, unique, mode, counter, known, taskRow, taskOrder, startButton, globalGroup, problem, modeText, admission, incident, incidentNotice, service, when, local, custom, history: history2, update, ai, addProject, bulk, views, appearance, agents, about, setup, attention, projects, controls, home, routeBody, resourceGroup, resourceCapability, resourceReceipt, capacity, workEntry, workPage, mountResources, notifications, notifyGroup, openCard, notedCard, project, filteredTasks, head, draw, wholeLines, settle, refresh, release, receive, measure, mount } = api;
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
          const present = execution.action("ui.render", (...args) => (ports.render || draw)(...args));
          const reports = require_reports().create({ send, draw: () => present(), defer });
          const notificationPanel = require_notification_panel().create({ view, reports, send, draw: () => present(), openCard });
          const { tick } = execution.module("ui.render", { tick: (...args) => (ports.render || draw)(...args) });
          const renderer = shell.create(menu);
          const color = (st) => ({ doing: "work", review: "review", ask: "ask", failed: "failed", ready: "ready" })[st] || "muted";
          const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
          const f4 = (...children) => h5(Fragment, null, ...children), raw = (html) => html ? h5("div", { style: "display:contents", dangerouslySetInnerHTML: { __html: html } }) : "", lines = (items) => items.flatMap((x5, i4) => i4 ? [h5("br"), x5] : [x5]);
          const runtimeName = (rt) => ({ "claude": "Claude", "codex": "Codex", "codex-app": "Codex app" })[rt] || rt || "Unknown runtime";
          const family = (rt) => /^codex/.test(rt || "") ? "Codex" : rt === "claude" ? "Claude" : rt || "Unknown runtime", grouped = (list) => {
            const m5 = /* @__PURE__ */ new Map();
            for (const x5 of list) m5.set(x5, (m5.get(x5) || 0) + 1);
            return [...m5].map(([k5, n3]) => n3 > 1 ? k5 + " (" + n3 + ")" : k5).join(" \xB7 ");
          };
          const signState = { "signed-in": "Signed in", expired: "Expired", rejected: "Rejected", "sign-in-required": "Sign in required", "not-installed": "Not installed", "check-failed": "Check failed", unknown: "Unknown" }, signAlert = ["expired", "rejected", "sign-in-required", "check-failed"];
          const actionErrors = /* @__PURE__ */ new Map();
          const routes = { hosts: "Hosts and resources", delivery: "Delivery", "agentq-shown": "Agentq shown", "agentq-setting-up": "Agentq setting up", "agentq-running": "Agentq running", "agentq-unknown": "Agentq unknown", work: "Work mode", recovery: "Recovery", stop: "Stop and hold", home: "Home", projects: "Projects", attention: "Attention", settings: "Settings", appearance: "Appearance", cadence: "Activity history", notifications: "Notifications", "notification-settings": "Notification delivery", agents: "Agents and access", services: "Services and support", about: "About and updates" };
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
