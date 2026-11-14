// DexNotas' use of highlight.js: highlight a text and hand back its coloured pieces as
// "start,end,scope;..." in UTF-16 units (the app turns them into byte ranges and colours).
import hljs from 'highlight.js';
const scopes = [];
const scopeIds = new Map();
function scopeId(s) {
  let id = scopeIds.get(s);
  if (id === undefined) { id = scopes.length; scopes.push(s); scopeIds.set(s, id); }
  return id;
}
globalThis.dexHighlight = function (code, language) {
  let result;
  try { result = hljs.highlight(code, { language, ignoreIllegals: true }); } catch (e) { return ''; }
  const out = [];
  let at = 0;
  const walk = (node, scope) => {
    if (typeof node === 'string') {
      const end = at + node.length;
      if (scope && end > at) out.push(at + ',' + end + ',' + scopeId(scope));
      at = end;
      return;
    }
    const s = node.scope || scope;
    for (const c of node.children) walk(c, node.scope ? s : scope);
  };
  walk(result._emitter.rootNode, null);
  return out.join(';');
};
globalThis.dexScopes = function () { return scopes.join('\n'); };
