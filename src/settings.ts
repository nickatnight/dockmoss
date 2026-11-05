import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  getSettings,
  isCommandError,
  saveSettings,
  type SettingsLimits,
  type SettingsView,
} from "./ipc";
import {
  exclusionRows,
  nextSettings,
  thresholdMessage,
  toFastForward,
  type ExclusionRow,
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
  groups: [
    byId("stages", HTMLFieldSetElement),
    byId("intensity-group", HTMLFieldSetElement),
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
  if (commandOnly && event.key.toLowerCase() === "w") {
    event.preventDefault();
    getCurrentWindow()
      .close()
      .catch((error: unknown) => {
        console.error("settings window did not close", error);
      });
  }
});

function show(view: SettingsView): void {
  const { settings, limits } = view;
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

/** Dock names and bundle IDs go into the DOM as text only. */
function exclusionItem(
  row: ExclusionRow,
  dockKnown: boolean,
  onChange: (excluded: boolean) => void,
): HTMLLIElement {
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = row.excluded;
  checkbox.addEventListener("change", () => onChange(checkbox.checked));
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = row.label;
  const label = document.createElement("label");
  label.append(checkbox, name);
  // Before the first Dock read, "not in the Dock" would be a guess.
  if (!row.inDock && dockKnown) {
    const note = document.createElement("span");
    note.className = "note";
    note.textContent = "Not in the Dock";
    label.append(note);
  }
  const item = document.createElement("li");
  item.append(label);
  return item;
}

function showIntensity(): void {
  page.intensityValue.textContent = `${page.intensity.value}%`;
}

function showThresholdError(message: string | null): void {
  page.stagesMessage.textContent = message ?? "";
  for (const input of dayInputs) {
    if (message === null) {
      input.removeAttribute("aria-invalid");
    } else {
      input.setAttribute("aria-invalid", "true");
    }
  }
}

function showSaveError(message: string | null): void {
  page.saveError.textContent = message ?? "";
  page.saveError.hidden = message === null;
}

/** An empty or partial entry reads as NaN, which Rust rejects. */
function enteredThresholds(): SettingsChange {
  return {
    field: "thresholds",
    thresholds: {
      fuzz: page.days.fuzz.valueAsNumber,
      patchy: page.days.patchy.valueAsNumber,
      overgrown: page.days.overgrown.valueAsNumber,
    },
  };
}

function reportUnsaved(
  change: SettingsChange,
  error: unknown,
  limits: SettingsLimits,
): void {
  const invalid = isCommandError(error) && error.kind === "invalid_settings";
  if (invalid && change.field === "thresholds") {
    showThresholdError(thresholdMessage(limits));
    return;
  }
  console.error("settings not saved", error);
  showSaveError("This change could not be saved. Try it again.");
}

async function start(): Promise<void> {
  const view = await getSettings();
  let saved = view.settings;
  show(view);
  for (const group of page.groups) {
    group.disabled = false;
  }

  // Saves run one at a time, each built from the settings the one before it
  // stored, so two quick changes cannot undo each other.
  let saving = Promise.resolve();
  const commit = (change: SettingsChange): void => {
    saving = saving.then(async () => {
      try {
        saved = await saveSettings(nextSettings(saved, change));
        showSaveError(null);
        if (change.field === "thresholds") {
          showThresholdError(null);
        }
      } catch (error) {
        reportUnsaved(change, error, view.limits);
      }
    });
  };

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
  const dockKnown = view.dockApps.length > 0;
  page.dockUnknown.hidden = dockKnown;
  page.excludedApps.replaceChildren(
    ...exclusionRows(view.dockApps, view.settings.excluded).map((row) =>
      exclusionItem(row, dockKnown, (excluded) =>
        commit({ field: "excluded", bundleId: row.bundleId, excluded }),
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
