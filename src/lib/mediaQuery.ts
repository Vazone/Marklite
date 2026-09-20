export function observeMediaQuery(
  media: MediaQueryList,
  onChange: (matches: boolean) => void
): () => void {
  const handleChange = (event: MediaQueryListEvent) => onChange(event.matches);
  media.addEventListener('change', handleChange);
  return () => media.removeEventListener('change', handleChange);
}
