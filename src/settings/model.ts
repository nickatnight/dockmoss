import type {
  BundleId,
  DockAppName,
  EnteredThresholds,
  ErrorKind,
  FastForward,
  LaunchAtLoginResult,
  LoginStatus,
  SaveSettingsResult,
  Settings,
  SettingsInput,
  SettingsLimits,
} from "../ipc";

export type SettingsChange =
  | { field: "thresholds"; thresholds: EnteredThresholds }
  | { field: "intensityPercent"; percent: number }
  | { field: "fastForward"; speed: FastForward }
  | { field: "excluded"; bundleId: BundleId; excluded: boolean };

export type PageChange =
  | SettingsChange
  | { field: "launchAtLogin"; enabled: boolean };

export type Outcome =
  | { ok: true }
  | { ok: false; error: { kind: ErrorKind | "ipc_failed" } };

export function nextSettings(
  saved: Settings,
  change: SettingsChange,
): SettingsInput {
  switch (change.field) {
    case "thresholds":
      return { ...saved, thresholds: { ...change.thresholds } };
    case "intensityPercent":
      return { ...saved, intensityPercent: change.percent };
    case "fastForward":
      return { ...saved, fastForward: change.speed };
    case "excluded": {
      const excluded = new Set(saved.excluded);
      if (change.excluded) {
        excluded.add(change.bundleId);
      } else {
        excluded.delete(change.bundleId);
      }
      return { ...saved, excluded: [...excluded].sort() };
    }
  }
}

export interface SettingsSaver {
  commit(change: SettingsChange): Promise<SaveSettingsResult>;
}

export function createSettingsSaver(
  initial: Settings,
  save: (settings: SettingsInput) => Promise<SaveSettingsResult>,
): SettingsSaver {
  let saved = initial;
  let previous: Promise<unknown> = Promise.resolve();
  return {
    commit(change) {
      const result = previous.then(async () => {
        const outcome = await save(nextSettings(saved, change));
        if (outcome.ok) {
          saved = outcome.settings;
        }
        return outcome;
      });
      previous = result.catch(() => undefined);
      return result;
    },
  };
}

export interface Feedback {
  thresholdError: boolean;
  exclusionLimit: boolean;
  unsaved: readonly string[];
  /** The last login change failed because macOS did not change the item,
   * which a retry does not fix. */
  loginNotChanged: boolean;
}

export const NO_FEEDBACK: Feedback = {
  thresholdError: false,
  exclusionLimit: false,
  unsaved: [],
  loginNotChanged: false,
};

function changedControl(change: PageChange): string {
  return change.field === "excluded"
    ? `excluded:${change.bundleId}`
    : change.field;
}

function rejected(result: Outcome): boolean {
  return !result.ok && result.error.kind === "invalid_settings";
}

export function nextFeedback(
  feedback: Feedback,
  change: PageChange,
  result: Outcome,
): Feedback {
  const control = changedControl(change);
  const loginNotChanged =
    change.field === "launchAtLogin"
      ? !result.ok && result.error.kind === "login_item_not_changed"
      : feedback.loginNotChanged;
  if (result.ok) {
    return {
      thresholdError:
        change.field === "thresholds" ? false : feedback.thresholdError,
      exclusionLimit:
        change.field === "excluded" ? false : feedback.exclusionLimit,
      unsaved: feedback.unsaved.filter((other) => other !== control),
      loginNotChanged,
    };
  }
  if (rejected(result) && change.field === "thresholds") {
    return { ...feedback, thresholdError: true };
  }
  if (rejected(result) && change.field === "excluded") {
    return { ...feedback, exclusionLimit: true };
  }
  const thresholdsPassedRules =
    change.field === "thresholds" && result.error.kind === "settings_not_saved";
  return {
    ...feedback,
    thresholdError: !thresholdsPassedRules && feedback.thresholdError,
    unsaved: [...new Set([...feedback.unsaved, control])].sort(),
    loginNotChanged,
  };
}

const TRY_AGAIN = "This change could not be saved. Try again.";
const CHECK_LOGIN_ITEMS =
  "macOS didn't change this. Check System Settings › General › Login Items.";

/** The save notice's text, or null when nothing failed. "Try again" only
 * for what a retry can fix; a login item macOS did not change points to
 * System Settings instead. */
export function saveNotice(feedback: Feedback): string | null {
  const retryable = feedback.unsaved.filter(
    (control) => !(control === "launchAtLogin" && feedback.loginNotChanged),
  );
  const loginNotChanged =
    feedback.loginNotChanged && feedback.unsaved.includes("launchAtLogin");
  const sentences = [
    ...(retryable.length > 0 ? [TRY_AGAIN] : []),
    ...(loginNotChanged ? [CHECK_LOGIN_ITEMS] : []),
  ];
  return sentences.length > 0 ? sentences.join(" ") : null;
}

/** Which messages to announce again: setting an alert to the text it already
 * shows is not a change a screen reader announces. */
export function reannounced(
  before: Feedback,
  change: PageChange,
  result: Outcome,
): { threshold: boolean; exclusionLimit: boolean; saveNotice: boolean } {
  const rejectedInPlace =
    rejected(result) &&
    (change.field === "thresholds" || change.field === "excluded");
  return {
    saveNotice: before.unsaved.length > 0 && !result.ok && !rejectedInPlace,
    threshold:
      before.thresholdError && change.field === "thresholds" && rejected(result),
    exclusionLimit:
      before.exclusionLimit && change.field === "excluded" && rejected(result),
  };
}

/** The status the box shows after a change: what macOS reported, or, when
 * the change failed, the state it was changed from. */
export function launchAtLoginShown(
  wanted: boolean,
  result: LaunchAtLoginResult,
): LoginStatus {
  if (result.ok) {
    return result.status;
  }
  return wanted ? "off" : "on";
}

export interface ExclusionRow {
  bundleId: BundleId;
  label: string;
  excluded: boolean;
  place: "dock" | "notInDock" | "unknown";
}

export function exclusionRows(
  dockApps: readonly DockAppName[] | null,
  excluded: readonly BundleId[],
): ExclusionRow[] {
  const excludedSet = new Set(excluded);
  const docked = (dockApps ?? []).map(
    (app): ExclusionRow => ({
      bundleId: app.bundleId,
      label: app.name,
      excluded: excludedSet.has(app.bundleId),
      place: "dock",
    }),
  );
  const inDock = new Set(docked.map((row) => row.bundleId));
  const strayPlace = dockApps === null ? "unknown" : "notInDock";
  const strays = [...excludedSet]
    .filter((bundleId) => !inDock.has(bundleId))
    .sort()
    .map(
      (bundleId): ExclusionRow => ({
        bundleId,
        label: bundleId,
        excluded: true,
        place: strayPlace,
      }),
    );
  return [...docked, ...strays];
}

export function enteredDays(valueAsNumber: number): number | null {
  return Number.isNaN(valueAsNumber) ? null : valueAsNumber;
}

export type DayField = "fuzz" | "patchy" | "overgrown";

/** The day fields a rejected entry got wrong, so only those are marked;
 * all three when no single field breaks a rule. */
export function invalidDays(
  days: EnteredThresholds,
  limits: SettingsLimits,
): DayField[] {
  const usable = (value: number | null): value is number =>
    value !== null &&
    Number.isInteger(value) &&
    value >= limits.minFuzzDays &&
    value <= limits.maxOvergrownDays;
  const after = (value: number | null, previous: number | null): boolean =>
    usable(value) && (!usable(previous) || value > previous);
  const wrong: DayField[] = [];
  if (!usable(days.fuzz)) wrong.push("fuzz");
  if (!after(days.patchy, days.fuzz)) wrong.push("patchy");
  if (!after(days.overgrown, days.patchy)) wrong.push("overgrown");
  return wrong.length > 0 ? wrong : ["fuzz", "patchy", "overgrown"];
}

export function exclusionLimitMessage(limits: SettingsLimits): string {
  return `At most ${limits.maxExclusions} apps can be excluded.`;
}

export function thresholdMessage(limits: SettingsLimits): string {
  return `Use whole days from ${limits.minFuzzDays} to ${limits.maxOvergrownDays}, each stage longer than the one before.`;
}

const FAST_FORWARD_SPEEDS: readonly FastForward[] = [1, 10, 100, 1000];

export function toFastForward(value: string): FastForward | null {
  return FAST_FORWARD_SPEEDS.find((speed) => String(speed) === value) ?? null;
}

function loginNote(status: LoginStatus | null): string | null {
  switch (status) {
    case null:
      return "macOS did not say whether this is on. To ask again, close and reopen Settings.";
    case "needsApproval":
      return "DockMoss needs your approval in System Settings › General › Login Items.";
    case "on":
    case "off":
      return null;
  }
}

export function launchAtLoginBox(status: LoginStatus | null): {
  checked: boolean;
  disabled: boolean;
  note: string | null;
} {
  return {
    checked: status === "on",
    disabled: status === null,
    note: loginNote(status),
  };
}

export interface LoginChangeDone {
  result: LaunchAtLoginResult;
  /** What the box shows now, or null when a later change has overtaken this one. */
  shown: LoginStatus | null;
}

async function settled(
  set: (wanted: boolean) => Promise<LaunchAtLoginResult>,
  wanted: boolean,
): Promise<LaunchAtLoginResult> {
  try {
    return await set(wanted);
  } catch (cause: unknown) {
    return { ok: false, error: { kind: "ipc_failed", cause } };
  }
}

/** Runs launch at login changes one at a time, in order. Only the latest
 * change's result sets the box; every result still reports. A `set` that
 * throws, or returns a promise that rejects, gives an `ipc_failed` result,
 * so a change never rejects. */
export function createLoginToggle(
  set: (wanted: boolean) => Promise<LaunchAtLoginResult>,
): (wanted: boolean) => Promise<LoginChangeDone> {
  let previous: Promise<unknown> = Promise.resolve();
  let latest = 0;
  return (wanted) => {
    latest += 1;
    const thisChange = latest;
    const done = previous.then(async () => {
      const result = await settled(set, wanted);
      return {
        result,
        shown: thisChange === latest ? launchAtLoginShown(wanted, result) : null,
      };
    });
    previous = done;
    return done;
  };
}
