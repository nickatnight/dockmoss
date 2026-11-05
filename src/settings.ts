import {
  getSettings,
  saveSettings,
  setLaunchAtLogin,
  type SettingsView,
} from "./ipc";
import {
  NO_FEEDBACK,
  createSettingsSaver,
  enteredDays,
  exclusionLimitMessage,
  exclusionRows,
  invalidDays,
  launchAtLoginShown,
  nextFeedback,
  reannounced,
  thresholdMessage,
  toFastForward,
  type DayField,
  type ExclusionRow,
  type Feedback,
  type Outcome,
  type PageChange,
  type SettingsChange,
} from "./settings/model";

function byId<T extends HTMLElement>(
  id: string,
  type: abstract new () => T,
): T {
  const found = document.getElementById(id);
  if (!(found instanceof type)) {
    throw new Error(`the settings page has no #${id}`);
  }
  return found;
}

const page = {
  saveError: byId("save-error", HTMLParagraphElement),
  launchAtLogin: byId("launch-at-login", HTMLInputElement),
  groups: [
    byId("stages", HTMLFieldSetElement),
    byId("fast-forward", HTMLFieldSetElement),
    byId("excluded-group", HTMLFieldSetElement),
  ],
  days: {
    fuzz: byId("fuzz", HTMLInputElement),
    patchy: byId("patchy", HTMLInputElement),
    overgrown: byId("overgrown", HTMLInputElement),
  },
  stagesMessage: byId("stages-message", HTMLParagraphElement),
  intensity: byId("intensity", HTMLInputElement),
  intensityValue: byId("intensity-value", HTMLOutputElement),
  dockUnknown: byId("dock-unknown", HTMLParagraphElement),
  excludedMessage: byId("excluded-message", HTMLParagraphElement),
  excludedApps: byId("excluded-apps", HTMLUListElement),
  speeds: Array.from(
    document.querySelectorAll<HTMLInputElement>('input[name="fast-forward"]'),
  ),
};

const dayInputs = Object.values(page.days);

// An Accessory app has no menu bar to carry ⌘W, so the page handles it.
document.addEventListener("keydown", (event) => {
  const commandOnly =
    event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
  if (commandOnly && event.code === "KeyW") {
    event.preventDefault();
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().close())
      .catch((error: unknown) => {
        console.error("settings window did not close", error);
      });
  }
});

function show(view: SettingsView): void {
  const { settings, limits } = view;
  page.launchAtLogin.checked = view.launchAtLogin === true;
  for (const input of dayInputs) {
    input.min = String(limits.minFuzzDays);
    input.max = String(limits.maxOvergrownDays);
  }
  page.days.fuzz.value = String(settings.thresholds.fuzz);
  page.days.patchy.value = String(settings.thresholds.patchy);
  page.days.overgrown.value = String(settings.thresholds.overgrown);
  page.intensity.value = String(settings.intensityPercent);
  showIntensity();
  for (const radio of page.speeds) {
    radio.checked = toFastForward(radio.value) === settings.fastForward;
  }
}

function exclusionItem(
  row: ExclusionRow,
  onChange: (excluded: boolean, undo: () => void) => void,
): HTMLLIElement {
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = row.excluded;
  checkbox.addEventListener("change", () => {
    const wanted = checkbox.checked;
    onChange(wanted, () => {
      checkbox.checked = !wanted;
    });
  });
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = row.label;
  const label = document.createElement("label");
  label.append(checkbox, name);
  if (row.place === "notInDock") {
    const note = document.createElement("span");
    note.className = "note";
    note.textContent = "Not in the Dock";
    label.append(note);
  }
  const item = document.createElement("li");
  item.append(label);
  // Focus scrolls the list only as far as the box; this brings in the whole
  // row, which holds the focus ring.
  checkbox.addEventListener("focus", () => {
    item.scrollIntoView({ block: "nearest" });
  });
  return item;
}

function showIntensity(): void {
  const percent = `${page.intensity.value}%`;
  page.intensityValue.textContent = percent;
  page.intensity.setAttribute("aria-valuetext", percent);
}

const refills = new Map<HTMLElement, number>();

/** Sets an alert's text. `again` empties it for a frame first, because
 * setting the text it already shows is not a change a screen reader
 * announces. */
function setAlert(element: HTMLElement, text: string, again: boolean): void {
  const pending = refills.get(element);
  if (pending !== undefined) {
    cancelAnimationFrame(pending);
    refills.delete(element);
  }
  if (!again) {
    element.textContent = text;
    return;
  }
  element.textContent = "";
  refills.set(
    element,
    requestAnimationFrame(() => {
      refills.delete(element);
      element.textContent = text;
    }),
  );
}

interface Messages {
  threshold: string;
  exclusionLimit: string;
}

function showFeedback(
  feedback: Feedback,
  messages: Messages,
  again: { threshold: boolean; exclusionLimit: boolean; saveNotice: boolean },
  wrongDays: readonly DayField[],
): void {
  setAlert(
    page.stagesMessage,
    feedback.thresholdError ? messages.threshold : "",
    again.threshold,
  );
  setAlert(
    page.excludedMessage,
    feedback.exclusionLimit ? messages.exclusionLimit : "",
    again.exclusionLimit,
  );
  for (const field of ["fuzz", "patchy", "overgrown"] as const) {
    if (feedback.thresholdError && wrongDays.includes(field)) {
      page.days[field].setAttribute("aria-invalid", "true");
    } else {
      page.days[field].removeAttribute("aria-invalid");
    }
  }
  showSaveError(
    feedback.unsaved.length > 0
      ? "This change could not be saved. Try again."
      : null,
    again.saveNotice,
  );
}

function showSaveError(message: string | null, again = false): void {
  page.saveError.hidden = message === null;
  setAlert(page.saveError, message ?? "", again);
}

function enteredThresholds(): SettingsChange {
  return {
    field: "thresholds",
    thresholds: {
      fuzz: enteredDays(page.days.fuzz.valueAsNumber),
      patchy: enteredDays(page.days.patchy.valueAsNumber),
      overgrown: enteredDays(page.days.overgrown.valueAsNumber),
    },
  };
}

async function start(): Promise<void> {
  const view = await getSettings();
  show(view);
  for (const group of page.groups) {
    group.disabled = false;
  }
  page.intensity.disabled = false;
  page.launchAtLogin.disabled = view.launchAtLogin === null;

  const saver = createSettingsSaver(view.settings, saveSettings);
  const messages = {
    threshold: thresholdMessage(view.limits),
    exclusionLimit: exclusionLimitMessage(view.limits),
  };
  let feedback = NO_FEEDBACK;
  let wrongDays: DayField[] = [];
  const report = (change: PageChange, result: Outcome): void => {
    const again = reannounced(feedback, change, result);
    feedback = nextFeedback(feedback, change, result);
    showFeedback(feedback, messages, again, wrongDays);
  };
  /** `undo` puts the control back when Rust refuses the value outright, as
   * for an exclusion past the cap, which no retry can store. */
  const commit = (change: SettingsChange, undo?: () => void): void => {
    saver
      .commit(change)
      .catch((error: unknown) => ({
        ok: false as const,
        error: { kind: "ipc_failed" as const, cause: error },
      }))
      .then((result) => {
        if (!result.ok) {
          console.error("settings not saved", result.error);
        }
        if (change.field === "thresholds") {
          wrongDays = invalidDays(change.thresholds, view.limits);
        }
        report(change, result);
        if (!result.ok && result.error.kind === "invalid_settings") {
          undo?.();
        }
      });
  };

  let loginChangeQueue = Promise.resolve();
  page.launchAtLogin.addEventListener("change", () => {
    const wanted = page.launchAtLogin.checked;
    loginChangeQueue = loginChangeQueue
      .then(() => setLaunchAtLogin(wanted))
      .then((result) => {
        if (!result.ok) {
          console.error("launch at login not changed", result.error);
        }
        page.launchAtLogin.checked = launchAtLoginShown(wanted, result);
        report({ field: "launchAtLogin", enabled: wanted }, result);
      });
  });

  for (const input of dayInputs) {
    input.addEventListener("change", () => commit(enteredThresholds()));
  }
  page.intensity.addEventListener("input", showIntensity);
  page.intensity.addEventListener("change", () =>
    commit({
      field: "intensityPercent",
      percent: page.intensity.valueAsNumber,
    }),
  );
  page.dockUnknown.hidden = view.dockApps !== null;
  page.excludedApps.replaceChildren(
    ...exclusionRows(view.dockApps, view.settings.excluded).map((row) =>
      exclusionItem(row, (excluded, undo) =>
        commit({ field: "excluded", bundleId: row.bundleId, excluded }, undo),
      ),
    ),
  );
  for (const radio of page.speeds) {
    radio.addEventListener("change", () => {
      const speed = toFastForward(radio.value);
      if (radio.checked && speed !== null) {
        commit({ field: "fastForward", speed });
      }
    });
  }
}

start().catch((error: unknown) => {
  console.error("settings did not load", error);
  showSaveError(
    "Settings could not be loaded. Close this window and try again.",
  );
});
