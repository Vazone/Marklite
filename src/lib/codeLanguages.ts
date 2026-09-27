import { LanguageDescription, LanguageSupport, StreamLanguage } from '@codemirror/language';

// Fixed local imports keep language bundles discoverable to Vite without
// shipping the entire language catalog or interpreting document text as a URL.
export const codeLanguages = [
  LanguageDescription.of({ name: 'JavaScript', alias: ['js', 'jsx'], load: async () =>
    (await import('@codemirror/lang-javascript')).javascript({ jsx: true }) }),
  LanguageDescription.of({ name: 'TypeScript', alias: ['ts', 'tsx'], load: async () =>
    (await import('@codemirror/lang-javascript')).javascript({ typescript: true, jsx: true }) }),
  LanguageDescription.of({ name: 'JSON', load: async () => (await import('@codemirror/lang-json')).json() }),
  LanguageDescription.of({ name: 'HTML', alias: ['htm'], load: async () => (await import('@codemirror/lang-html')).html() }),
  LanguageDescription.of({ name: 'CSS', load: async () => (await import('@codemirror/lang-css')).css() }),
  LanguageDescription.of({ name: 'Rust', alias: ['rs'], load: async () => (await import('@codemirror/lang-rust')).rust() }),
  LanguageDescription.of({ name: 'Python', alias: ['py'], load: async () => (await import('@codemirror/lang-python')).python() }),
  LanguageDescription.of({ name: 'Shell', alias: ['sh', 'bash', 'zsh'], load: async () =>
    new LanguageSupport(StreamLanguage.define((await import('@codemirror/legacy-modes/mode/shell')).shell)) }),
  LanguageDescription.of({ name: 'SQL', load: async () => (await import('@codemirror/lang-sql')).sql() }),
  LanguageDescription.of({ name: 'YAML', alias: ['yml'], load: async () => (await import('@codemirror/lang-yaml')).yaml() })
];

export function findCodeLanguage(info: string): LanguageDescription | null {
  const name = info.trim().split(/\s+/, 1)[0];
  return LanguageDescription.matchLanguageName(codeLanguages, name, false);
}
