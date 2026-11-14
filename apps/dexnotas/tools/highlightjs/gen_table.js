// Writes src/syntax/hljs_langs.rs: the highlight.js languages DexNotas offers (the ones its
// tree-sitter grammars don't already cover), with the extensions that select them and their line
// comment. Run by build.sh.
const fs = require('fs');
const path = require('path');
const hljs = require('highlight.js');

// Languages DexNotas colours with tree-sitter (src/syntax/mod.rs), or that are not a language.
const COVERED = new Set([
  'bash', 'c', 'cpp', 'csharp', 'css', 'go', 'java', 'javascript', 'json', 'lua', 'markdown', 'php',
  'powershell', 'python', 'ruby', 'rust', 'scss', 'sql', 'swift', 'typescript', 'xml', 'yaml', 'ini',
  'plaintext',
]);

// The file extensions of each language (lower case, without the dot). highlight.js' aliases mix
// extensions with names, and some of them are binary formats (xls) or clash: this list is chosen.
const EXTENSIONS = {
  '1c': ['bsl'], abnf: ['abnf'], accesslog: [], actionscript: ['as'], ada: ['adb', 'ads', 'ada'],
  angelscript: ['angelscript'], apache: ['apacheconf', 'htaccess'], applescript: ['applescript'],
  arcade: ['arcade'], arduino: ['pde'], armasm: ['arm'], asciidoc: ['adoc', 'asciidoc', 'asc'],
  aspectj: ['aj'], autohotkey: ['ahk'], autoit: ['au3'], avrasm: [], awk: ['awk'], axapta: ['xpp'],
  basic: ['bas'], bnf: ['bnf'], brainfuck: ['bf'], cal: ['cal'], capnproto: ['capnp'], ceylon: ['ceylon'],
  clean: ['icl', 'dcl'], clojure: ['clj', 'cljs', 'cljc', 'edn'], 'clojure-repl': [], cmake: ['cmake'],
  coffeescript: ['coffee', 'cson', 'iced'], coq: [], cos: ['cls'], crmsh: ['crm', 'pcmk'],
  crystal: ['cr'], csp: [], d: ['d', 'di'], dart: ['dart'], delphi: ['pas', 'dpr', 'dfm', 'lpr', 'pp'],
  diff: ['diff', 'patch'], django: ['jinja', 'j2', 'jinja2', 'djhtml'], dns: ['zone'],
  dockerfile: ['dockerfile'], dos: ['bat', 'cmd'], dsconfig: [], dts: ['dts', 'dtsi'], dust: ['dust', 'dst'],
  ebnf: ['ebnf'], elixir: ['ex', 'exs'], elm: ['elm'], erb: ['erb'], 'erlang-repl': [], erlang: ['erl', 'hrl'],
  excel: [], fix: [], flix: ['flix'], fortran: ['f90', 'f95', 'f03', 'f08', 'f', 'for', 'f77'],
  freedesktop: ['desktop', 'service', 'timer', 'socket', 'mount', 'target'], fsharp: ['fs', 'fsi', 'fsx'],
  gams: ['gms'], gauss: ['gss'], gcode: ['gcode'], gherkin: ['feature'], glsl: ['glsl', 'vert', 'frag', 'geom'],
  gml: ['gml'], golo: ['golo'], gradle: ['gradle'], graphql: ['graphql', 'gql'], groovy: ['groovy', 'gvy', 'gy'],
  haml: ['haml'], handlebars: ['hbs', 'handlebars', 'mustache'], haskell: ['hs', 'lhs'], haxe: ['hx'],
  hsp: ['hsp'], http: ['http'], hy: ['hy'], inform7: ['ni', 'i7x'], irpf90: [], isbl: ['isbl'],
  'jboss-cli': [], julia: ['jl'], 'julia-repl': [], kotlin: ['kt', 'kts', 'ktm'], lasso: ['lasso'],
  latex: ['tex', 'sty', 'ltx', 'bib'], ldif: ['ldif'], leaf: ['leaf'], less: ['less'], lisp: ['lisp', 'lsp', 'el'],
  livecodeserver: ['lc'], livescript: ['ls'], llvm: ['ll'], lsl: ['lsl'], makefile: ['mk', 'mak', 'make'],
  mathematica: ['wl', 'mma', 'wls'], matlab: [], maxima: ['mac'], mel: ['mel'], mercury: ['moo'],
  mipsasm: ['mips'], mizar: ['miz'], mojolicious: [], monkey: ['monkey'], moonscript: ['moon'],
  n1ql: ['n1ql'], nestedtext: ['nt'], nginx: ['nginxconf'], nim: ['nim', 'nims', 'nimble'], nix: ['nix'],
  'node-repl': [], nsis: ['nsi', 'nsh'], objectivec: ['m', 'mm'], ocaml: ['ml', 'mli'], openscad: ['scad'],
  oxygene: ['oxygene'], parser3: [], perl: ['pl', 'pm', 'pod', 'perl'], mojolicious: ['ep'], pf: [],
  pgsql: ['pgsql'], 'php-template': [], pony: ['pony'], processing: [], profile: [], prolog: ['pro', 'prolog'],
  properties: ['properties'], protobuf: ['proto'], puppet: [], purebasic: ['pb', 'pbi'],
  'python-repl': ['pycon'], q: ['q', 'k'], qml: ['qml'], r: ['r', 'rmd'], reasonml: ['re', 'rei'], rib: ['rib'],
  roboconf: [], routeros: ['rsc'], rsl: ['rsl'], ruleslanguage: [], sas: ['sas'], scala: ['scala', 'sc'],
  scheme: ['scm', 'ss', 'rkt'], scilab: ['sci', 'sce'], shell: ['console'], smali: ['smali'],
  smalltalk: ['st'], sml: ['sml', 'sig'], sqf: ['sqf'], stan: ['stan'], stata: ['do', 'ado'],
  step21: ['stp', 'step', 'p21'], stylus: ['styl'], subunit: [], taggerscript: [], tap: ['tap'],
  tcl: ['tcl', 'tk'], thrift: ['thrift'], tp: [], twig: ['twig'], vala: ['vala', 'vapi'], vbnet: ['vb'],
  vbscript: ['vbs'], 'vbscript-html': [], verilog: ['v', 'sv', 'svh'], vhdl: ['vhd', 'vhdl'],
  vim: ['vim', 'vimrc'], wasm: ['wat', 'wast'], wren: ['wren'], x86asm: ['asm', 'nasm', 's'],
  xl: ['xl'], xquery: ['xq', 'xqm', 'xquery', 'xqy'], zephir: ['zep'],
};

// Whole file names that select a language.
const FILE_NAMES = {
  dockerfile: ['dockerfile', 'containerfile'], makefile: ['makefile', 'gnumakefile'], cmake: ['cmakelists.txt'],
  apache: ['.htaccess'], nginx: ['nginx.conf'], pf: ['pf.conf'], vim: ['.vimrc', '_vimrc'],
  ruby: [], gradle: [], properties: [],
};

// Line comments the detection below gets wrong or misses.
const COMMENTS = {
  r: '#', vbnet: "'", basic: "'", dos: 'REM', properties: '#', haskell: '--', tcl: '#', gherkin: '#',
  mipsasm: '#', routeros: '#', smalltalk: null, sas: null, sml: null, ocaml: null, shell: null,
  nestedtext: '#', http: null, inform7: null, maxima: null, mathematica: null,
};

/** A line comment from the language's top-level comment modes (`//`, `#`, `--`, `;`…). */
function lineComment(lang) {
  const found = [];
  const look = (modes) => {
    for (const m of modes || []) {
      if (!m || typeof m !== 'object') continue;
      if ((m.scope === 'comment' || m.className === 'comment') && (m.end === '$' || String(m.end) === '/$/')) {
        let b = m.begin;
        if (b instanceof RegExp) b = b.source;
        if (Array.isArray(b)) continue;
        if (typeof b !== 'string') continue;
        const plain = b.replace(/\\(.)/g, '$1');
        if (/^[^\s\w()[\]|?*+{}^$.]{1,3}$/.test(plain) || /^REM$/i.test(plain)) found.push(plain);
      }
      if (m.variants) look(m.variants);
    }
  };
  look(lang.contains);
  return found[0] || null;
}

const rust = (s) => JSON.stringify(s);
const rows = [];
for (const id of hljs.listLanguages()) {
  if (COVERED.has(id)) continue;
  const lang = hljs.getLanguage(id);
  const exts = EXTENSIONS[id];
  if (exts === undefined) throw new Error(`no extension list for ${id}`);
  const aliases = [id, ...(lang.aliases || [])].map((a) => a.toLowerCase());
  const comment = id in COMMENTS ? COMMENTS[id] : lineComment(lang.rawDefinition ? lang.rawDefinition(hljs) : lang);
  rows.push({ id, name: lang.name, exts, aliases, names: FILE_NAMES[id] || [], comment });
}
rows.sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }));

let out = `// Generated by tools/highlightjs/gen_table.js from highlight.js ${require('highlight.js/package.json').version}: do not edit.
// The highlight.js languages DexNotas offers besides its tree-sitter ones, by name.

/// A highlight.js language.
pub struct Lang {
    /// highlight.js' name for it.
    pub id: &'static str,
    /// The name shown.
    pub name: &'static str,
    /// The file extensions that select it.
    pub exts: &'static [&'static str],
    /// Whole file names that select it (lower case).
    pub file_names: &'static [&'static str],
    /// What a code block in Markdown may call it.
    pub aliases: &'static [&'static str],
    /// Its line comment, for Toggle comment.
    pub comment: Option<&'static str>,
}

pub static LANGS: &[Lang] = &[
`;
for (const r of rows) {
  out += `    Lang { id: ${rust(r.id)}, name: ${rust(r.name)}, exts: &[${r.exts.map(rust).join(', ')}], file_names: &[${r.names.map(rust).join(', ')}], aliases: &[${r.aliases.map(rust).join(', ')}], comment: ${r.comment ? `Some(${rust(r.comment)})` : 'None'} },\n`;
}
out += '];\n';
fs.writeFileSync(path.join(__dirname, '../../src/syntax/hljs_langs.rs'), out);
console.log(`${rows.length} languages`);
