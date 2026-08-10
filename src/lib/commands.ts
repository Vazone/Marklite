export type CommandItem = {
  id: string;
  title: string;
  category: string;
  shortcut?: string;
  action: () => void;
};
