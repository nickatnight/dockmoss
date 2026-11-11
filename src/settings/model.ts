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

interface IpcFailed {
  ok: false;
  error: { kind: "ipc_failed"; cause: unknown };
}

/** A throw or a rejection from `call` becomes an `ipc_failed` result. */
async function settled<Arg, Result>(
  call: (arg: Arg) => Promise<Result>,
  arg: Arg,
): Promise<Result | IpcFailed> {
  try {
    return await call(arg);
  } catch (cause: unknown) {
    return { ok: false, error: { kind: "ipc_failed", cause } };
  }
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
        const outcome = await settled(save, nextSettings(saved, change));
        if (outcome.ok) {
          saved = outcome.settings;
        }
        return outcome;
      });
      previous = result;
      return result;
    },
  };
}

export interface Feedback {
  thresholdError: boolean;
  exclusionLimit: boolean;
  unsaved: readonly string[];
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
/** No-break spaces keep the path on one line. */
const LOGIN_ITEMS_PATH = "System\u00a0Settings\u00a0›\u00a0General\u00a0›\u00a0Login\u00a0Items";
const CHECK_LOGIN_ITEMS = `macOS did not change launch at login. Check ${LOGIN_ITEMS_PATH}.`;

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

export function launchAtLoginShown(
  before: LoginStatus,
  result: LaunchAtLoginResult,
): LoginStatus {
  return result.ok ? result.status : before;
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
      return `DockMoss needs your approval in ${LOGIN_ITEMS_PATH}.`;
    case "on":
    case "off":
      return null;
  }
}

/** Only the approval note reports a change's result; a failure has the red
 * notice. Announced again even when it already shows. */
export function announceLoginNote(result: LaunchAtLoginResult): boolean {
  return result.ok && result.status === "needsApproval";
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

export function createLoginToggle(
  set: (wanted: boolean) => Promise<LaunchAtLoginResult>,
  loaded: LoginStatus,
): (wanted: boolean) => Promise<LoginChangeDone> {
  let previous: Promise<unknown> = Promise.resolve();
  let latest = 0;
  let reported = loaded;
  return (wanted) => {
    latest += 1;
    const thisChange = latest;
    const done = previous.then(async () => {
      const result = await settled(set, wanted);
      reported = launchAtLoginShown(reported, result);
      return { result, shown: thisChange === latest ? reported : null };
    });
    previous = done;
    return done;
  };
}
