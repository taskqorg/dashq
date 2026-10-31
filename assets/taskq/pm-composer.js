"use strict";
var TaskqPmComposer = (() => {
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

  // src/pm-composer.js
  var require_pm_composer = __commonJS({
    "src/pm-composer.js"(exports, module) {
      var { h: h2, render } = (init_preact(), __toCommonJS(preact_exports));
      var ui = require_ui_runtime();
      var { execution } = ui;
      var measured = /* @__PURE__ */ new WeakMap();
      var api = execution.module("pm-composer", {
        request: function(context, text, op, now) {
          if (!text.trim() || new TextEncoder().encode(text).length > 65536) throw Error("Write 1\u201365536 UTF-8 bytes");
          const target = Object.fromEntries(["installation_id", "generation", "project_id", "runtime_id"].map((k2) => [k2, context.target?.[k2]]));
          const request = { schema: "taskq.pm/1", actor_id: context.actor_id, op_id: op, grant_id: context.grant_id, revision: context.revision, target, issued_at: now, expires_at: now + 300, text };
          for (const id of [request.actor_id, op, request.grant_id, request.revision, context.target?.installation_id, context.target?.project_id, context.target?.runtime_id]) if (!/^[A-Za-z0-9_-]{1,64}$/.test(id || "")) throw Error("PM target or authority unavailable");
          if (!Number.isSafeInteger(context.target.generation) || context.target.generation < 1 || !Number.isSafeInteger(now) || now < 0) throw Error("PM identity or clock unavailable");
          return JSON.parse(JSON.stringify(request));
        },
        receipt: function(value, request) {
          if (!value || value.schema !== "taskq.pm/1" || !["accepted", "rejected", "stale", "unknown"].includes(value.outcome) || ["actor_id", "op_id", "revision"].some((k2) => value[k2] !== request[k2])) return { outcome: "unknown", reason: "No matching authoritative receipt" };
          if (value.outcome === "accepted" && (!/^[a-f0-9]{64}$/.test(value.request_id || "") || !value.native?.host || !value.native?.runtime)) return { outcome: "unknown", reason: "Native request identity unavailable" };
          return JSON.parse(JSON.stringify(value));
        },
        handoff: function(native, local = false) {
          if (!native) return null;
          const url = native.url, safe = typeof url === "string" && (/^https:\/\/claude\.ai\/code\/session_[A-Za-z0-9]+$/.test(url) || local && /^codex:\/\/threads\/[A-Za-z0-9-]+$/.test(url));
          return h2("div", { class: "pm-handoff" }, h2("p", { role: "status" }, (native.kind || native.state || "unknown") + ": " + (native.reason || native.next || (native.state === "launched" && native.session ? "Continue in the native session" : "Native outcome unknown"))), h2("p", { class: "meta" }, (native.runtime || "Runtime unknown") + " \xB7 " + (native.host || "Host unknown") + (native.session ? " \xB7 " + native.session : "")), safe && h2("a", { href: url, "data-url": url, class: "native-session" }, "Open session"));
        },
        grow: function(root) {
          const window = root.ownerDocument.defaultView;
          for (const input of root.querySelectorAll(".conversation-composer textarea")) {
            const style = window.getComputedStyle(input), width = input.getBoundingClientRect().width, key = JSON.stringify([input.value, width, style.font, style.lineHeight, style.minHeight]);
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
          return h2("div", { class: "conversation-composer", onPaste, onDrop, onDragOver }, before, h2("label", { for: id, class: "sr-only" }, label), h2("textarea", { id, "data-c-field": field, rows: 3, value: text, disabled, "aria-describedby": id === "conversation-draft" ? "conversation-reason" : id + "-reason", placeholder, onInput }), files, tools, h2("p", { id: id === "conversation-draft" ? "conversation-reason" : id + "-reason", class: "meta" }, reason), after);
        },
        mount: function(el, adapter) {
          let draft = adapter.load() || { text: "" }, pending = false, timer, serial = 0, invalid = false;
          if (draft.request) {
            try {
              const r2 = draft.request, expected = api.request(r2, r2.text, r2.op_id, r2.issued_at);
              if (r2.schema !== expected.schema || r2.expires_at !== expected.expires_at) throw Error("Invalid saved request");
              draft.request = expected;
              if (draft.receipt) draft.receipt = api.receipt(draft.receipt, expected);
            } catch (_2) {
              invalid = true;
              draft.receipt = { outcome: "unknown", reason: "Saved request invalid; preserve it for host reconciliation" };
            }
          } else if (draft.receipt?.native) draft.receipt = { outcome: "unknown", reason: "No bound request for this receipt" };
          const local = execution.module("pm-composer.instance", {
            paint: function() {
              render(h2("section", { class: "pm-composer", "aria-label": "PM request" }, api.composer({ text: draft.text, disabled: !!draft.request, reason: pending ? "Checking delivery\u2026" : draft.receipt?.reason || adapter.context().reason || "Replies stay in the native session.", onInput: (e2) => {
                draft.text = e2.currentTarget.value;
                adapter.save(draft);
                local.paint();
              }, tools: h2("div", { class: "conversation-tools" }, h2("button", { disabled: pending || invalid || !!draft.request || !draft.text.trim() || !!adapter.context().reason, onClick: () => local.send(false) }, "Send"), draft.request && h2("button", { disabled: pending || invalid || !adapter.reconcile, onClick: () => local.send(true) }, "Check delivery")) }), draft.receipt && h2("p", { role: "status" }, draft.receipt.outcome), api.handoff(draft.receipt?.native)), el);
              api.grow(el);
            },
            send: function(reconcile) {
              if (invalid || pending || reconcile && !draft.request || !reconcile && draft.request) return;
              if (!reconcile) {
                try {
                  const next = { ...draft, request: api.request(adapter.context(), draft.text, adapter.id(), adapter.now()) };
                  adapter.save(next);
                  draft = next;
                } catch (_2) {
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
  return require_pm_composer();
})();
