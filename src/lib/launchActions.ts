/**
 * Shared "launch the selected instance" entry point for the Home hero card and
 * the bottom launch dock. Keeping one implementation means the missing-account
 * flow (offline username prompt) cannot drift between the two launch surfaces.
 */
import type { InstanceDetail } from "../ipc/types";
import { authStore } from "../stores/auth";
import { launchStore } from "../stores/launch";
import { promptText, toast } from "../stores/ui";
import { t } from "../i18n";

export async function launchInstance(
  profile: InstanceDetail | null,
  navigate: (href: string) => void
): Promise<void> {
  if (!profile) {
    navigate("/download#install");
    return;
  }
  if (!authStore.session()) {
    const username = await promptText({
      title: t("accounts.offline.title"),
      label: t("home.offlinePrompt"),
      placeholder: t("accounts.offline.name"),
    });
    if (!username) {
      navigate("/accounts");
      return;
    }
    const res = await authStore.startOfflineLogin(username);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return;
    }
  }
  const result = await launchStore.start(profile.summary.key, profile.summary.display_name);
  if (!result.ok) toast({ message: result.message ?? "", tone: "error" });
}
