import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { artifactDigest, isPublicAsset, validateRelease } from './manifest.mjs';

function newer(version, previous) {
  const left = version.split('.').map(BigInt);
  const right = previous.split('.').map(BigInt);
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] > right[i];
  return false;
}

// gh is injected so failure ordering can be tested without changing GitHub state.
export async function publish(release, directory, gh, verifyAssets) {
  validateRelease(release);
  if (typeof verifyAssets !== 'function') throw new Error('Release signature and manifest verification is required');
  await verifyAssets(release, directory);
  const repo = release.repository;
  const tag = `v${release.version}`;
  const guard = async () => {
    const pages = JSON.parse(await gh(['api', `repos/${repo}/releases`, '--paginate', '--slurp']));
    const releases = pages.flat();
    if (releases.some(item => item.tag_name === tag)) throw new Error('Release already exists; refusing to overwrite');
    for (const item of releases) {
      if (!item.draft && !item.prerelease && /^v\d+\.\d+\.\d+$/.test(item.tag_name) && !newer(release.version, item.tag_name.slice(1))) {
        throw new Error('Stable release version must increase');
      }
    }
  };
  await guard();
  const tags = JSON.parse(await gh(['api', `repos/${repo}/git/matching-refs/tags/${tag}`]));
  if (tags.some(item => item.ref === `refs/tags/${tag}`)) throw new Error('Tag already exists; use a new version');
  const staged = await readdir(directory);
  if (!staged.includes('latest.json') || !staged.includes('release-evidence.json') || !staged.includes('SHA256SUMS.txt')) throw new Error('Missing assembled release metadata');
  const names = staged.filter(isPublicAsset).sort();
  const assets = [];
  for (const name of names) assets.push({ name, ...await artifactDigest(directory, name) });
  const manifest = JSON.parse(await readFile(join(directory, 'latest.json'), 'utf8'));
  if (manifest.version !== release.version) throw new Error('Manifest version mismatch');
  // Create an unpublished draft first. Any later error leaves the existing stable release intact.
  const draft = JSON.parse(await gh(['api', `repos/${repo}/releases`, '--method', 'POST', '--input', '-'], {
    tag_name: tag, target_commitish: release.commit, name: `MarkLite ${tag}`,
    body: release.notes, draft: true, prerelease: false, make_latest: 'false'
  }));
  await gh(['release', 'upload', tag, ...names.map(name => join(directory, name)), '--repo', repo]);
  const uploaded = JSON.parse(await gh(['api', `repos/${repo}/releases/${draft.id}/assets`, '--paginate', '--slurp'])).flat();
  if (uploaded.length !== assets.length || assets.some(local => {
    const matching = uploaded.filter(remote => remote.name === local.name);
    return matching.length !== 1 || matching[0].state !== 'uploaded' || matching[0].size !== local.bytes || matching[0].digest !== `sha256:${local.sha256}`;
  })) throw new Error('Uploaded release asset digest/size set mismatch');
  const current = JSON.parse(await gh(['api', `repos/${repo}/releases`, '--paginate', '--slurp'])).flat();
  if (current.some(item => item.id !== draft.id && !item.draft && !item.prerelease && /^v\d+\.\d+\.\d+$/.test(item.tag_name) && !newer(release.version, item.tag_name.slice(1)))) throw new Error('A newer stable release appeared during upload');
  await gh(['api', `repos/${repo}/releases/${draft.id}`, '--method', 'PATCH', '--input', '-'], {
    draft: false, prerelease: false, make_latest: 'true'
  });
}
