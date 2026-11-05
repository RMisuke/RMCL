declare module "virtual:changelog" {
  import type { ChangelogEntry } from "./lib/changelog";

  const entries: ChangelogEntry[];
  export default entries;
}
