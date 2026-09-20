import { execFile } from 'node:child_process';
import { access, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';
import { normalizeMermaidSvgDocument } from '../src/shared/mermaidSvgNormalization.mjs';

const exec = promisify(execFile);
const root = resolve(import.meta.dirname, '..');
const fixtures = JSON.parse(await readFile(join(root, 'src', 'shared', 'mermaid-fixtures.json'), 'utf8'));
const runtime = join(root, 'node_modules', 'mermaid', 'dist', 'mermaid.min.js');
const evidencePath = join(root, 'Review', 'diagram-renderer-conformance-0092.json');
const workspace = await mkdtemp(join(tmpdir(), 'marklite-diagram-conformance-'));
const artifactsPath = join(workspace, 'artifacts.json');
const pagePath = join(workspace, 'fixtures.html');

const edgeCandidates = [
  process.env.PROGRAMFILES && join(process.env.PROGRAMFILES, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
  process.env['PROGRAMFILES(X86)'] && join(process.env['PROGRAMFILES(X86)'], 'Microsoft', 'Edge', 'Application', 'msedge.exe')
].filter(Boolean);
const chromeCandidates = [
  process.env.PROGRAMFILES && join(process.env.PROGRAMFILES, 'Google', 'Chrome', 'Application', 'chrome.exe'),
  process.env['PROGRAMFILES(X86)'] && join(process.env['PROGRAMFILES(X86)'], 'Google', 'Chrome', 'Application', 'chrome.exe')
].filter(Boolean);

async function findBrowsers() {
  const browsers = [];
  for (const [name, candidates] of [['Microsoft Edge', edgeCandidates], ['Google Chrome', chromeCandidates]]) {
    for (const candidate of candidates) {
      try {
        await access(candidate);
        browsers.push({ name, path: candidate });
        break;
      } catch {}
    }
  }
  if (!browsers.length) throw new Error('No supported Chromium browser was found');
  return browsers;
}

function decodeResult(document) {
  const match = document.match(/<pre id="result">([^<]+)<\/pre>/);
  if (!match || match[1] === 'pending') {
    throw new Error(`Browser did not complete the Mermaid fixture run (result=${match?.[1] ?? 'missing'}, documentBytes=${Buffer.byteLength(document)})`);
  }
  return JSON.parse(Buffer.from(match[1], 'base64').toString('utf8'));
}

try {
  const browsers = await findBrowsers();
  const fixtureJson = JSON.stringify(fixtures).replaceAll('</script', '<\\/script');
  const normalizationSource = normalizeMermaidSvgDocument.toString();
  const html = `<!doctype html><html><body><pre id="result">pending</pre>
<script src="${pathToFileURL(runtime).href}"></script><script>
const fixtures=${fixtureJson};
const normalizeMermaidSvgDocument=${normalizationSource};
function base64Utf8(value){const bytes=new TextEncoder().encode(value);let binary='';for(const byte of bytes)binary+=String.fromCharCode(byte);return btoa(binary)}
(async()=>{const results=[];try{
mermaid.initialize({startOnLoad:false,securityLevel:'strict',theme:'default',look:'classic',layout:'dagre',htmlLabels:true,fontFamily:'Arial, system-ui, sans-serif',logLevel:'fatal',flowchart:{htmlLabels:true,defaultRenderer:'dagre-wrapper'}});
for(let index=0;index<fixtures.length;index++){const fixture=fixtures[index];const rendered=await mermaid.render('marklite-fixture-'+index,fixture.source);const parsed=new DOMParser().parseFromString(rendered.svg,'image/svg+xml');const containsMathMl=Boolean(parsed.querySelector('math'));const containsKatex=rendered.svg.includes('katex');const root=normalizeMermaidSvgDocument(parsed.documentElement);const svgUtf8=new XMLSerializer().serializeToString(root);const viewBox=(root.getAttribute('viewBox')||'').trim().split(/[\\s,]+/).map(Number);results.push({id:fixture.id,source:fixture.source,svgUtf8,viewBox,containsMathMl,containsKatex});}
document.getElementById('result').textContent=base64Utf8(JSON.stringify({ok:true,results}));
}catch(error){document.getElementById('result').textContent=base64Utf8(JSON.stringify({ok:false,error:error instanceof Error?error.stack:String(error)}));}})();
</script></body></html>`;
  await writeFile(pagePath, html, 'utf8');
  let browserDocument = '';
  let browserName = '';
  for (const browser of browsers) {
    const { stdout } = await exec(browser.path, [
      '--headless=new',
      '--disable-gpu',
      '--allow-file-access-from-files',
      `--user-data-dir=${join(workspace, `${browser.name.replaceAll(' ', '-').toLowerCase()}-profile`)}`,
      '--virtual-time-budget=30000',
      '--dump-dom',
      pathToFileURL(pagePath).href
    ], { maxBuffer: 64 * 1024 * 1024, windowsHide: true });
    if (stdout) {
      browserDocument = stdout;
      browserName = browser.name;
      break;
    }
  }
  if (!browserDocument) throw new Error('Supported Chromium browsers produced no DOM');
  const browserResult = decodeResult(browserDocument);
  if (!browserResult.ok) throw new Error(browserResult.error);
  const svgElements = [...new Set(browserResult.results.flatMap((item) =>
    [...item.svgUtf8.matchAll(/<\/?([A-Za-z][A-Za-z0-9]*)\b/g)].map((match) => match[1])
  ))].sort();
  console.log(`SVG elements: ${svgElements.join(', ')}`);
  await writeFile(
    artifactsPath,
    JSON.stringify(browserResult.results.map(({ id, source, svgUtf8, viewBox }) => ({ id, source, svgUtf8, viewBox }))),
    'utf8'
  );
  const native = await exec('cargo', [
    'run', '--quiet', '--manifest-path', join(root, 'src-tauri', 'Cargo.toml'),
    '--example', 'validate_mermaid_fixtures', '--features', 'diagram-tooling', '--', artifactsPath
  ], { cwd: root, maxBuffer: 8 * 1024 * 1024, windowsHide: true });
  const evidence = {
    schemaVersion: 1,
    taskCard: 'TaskCards/done/0092-implement-bounded-local-diagram-renderer.md',
    rendererId: 'mermaid-offline-11.17.2',
    browser: `${browserName} headless`,
    fixtureCount: browserResult.results.length,
    nativePolicyResult: native.stdout.trim(),
    totalSvgBytes: browserResult.results.reduce((total, item) => total + new TextEncoder().encode(item.svgUtf8).byteLength, 0),
    fixtures: browserResult.results.map((item) => ({
      id: item.id,
      svgBytes: new TextEncoder().encode(item.svgUtf8).byteLength,
      viewBox: item.viewBox,
      containsMathMl: item.containsMathMl,
      containsKatex: item.containsKatex
    }))
  };
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify(evidence, null, 2));
} finally {
  await rm(workspace, { recursive: true, force: true });
}
