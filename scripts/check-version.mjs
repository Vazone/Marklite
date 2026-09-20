import { fileURLToPath } from 'node:url';
import { createRepositoryIndex } from './lib/governance/repository-index.mjs';
import { assertVersionContract } from './lib/governance/version-contract.mjs';

const repositoryRoot = fileURLToPath(new URL('../', import.meta.url));
const repository = createRepositoryIndex(repositoryRoot);
const releaseTag = process.env.RELEASE_TAG ?? process.env.GITHUB_REF_NAME;
const expected = assertVersionContract(repository, releaseTag);

console.log(`Version check passed: ${expected}${releaseTag ? ` (${releaseTag})` : ''}`);
