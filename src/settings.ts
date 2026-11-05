import { getSettings, saveSettings, type SettingsView } from "./ipc";
import {
  NO_FEEDBACK,
  createSettingsSaver,
  enteredDays,
  exclusionRows,
  nextFeedback,
  thresholdMessage,
  toFastForward,
  type ExclusionRow,
  type Feedback,
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
  if (row.place === "notInDock") {
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

function showFeedback(feedback: Feedback, message: string): void {
  page.stagesMessage.textContent = feedback.thresholdError ? message : "";
  for (const input of dayInputs) {
    if (feedback.thresholdError) {
      input.setAttribute("aria-invalid", "true");
    } else {
      input.removeAttribute("aria-invalid");
    }
  }
  showSaveError(
    feedback.unsaved.length > 0
      ? "This change could not be saved. Try it again."
      : null,
  );
}

function showSaveError(message: string | null): void {
  page.saveError.textContent = message ?? "";
  page.saveError.hidden = message === null;
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

  const saver = createSettingsSaver(view.settings, saveSettings);
  const message = thresholdMessage(view.limits);
  let feedback = NO_FEEDBACK;
  const commit = (change: SettingsChange): void => {
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
        feedback = nextFeedback(feedback, change, result);
        showFeedback(feedback, message);
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
  page.dockUnknown.hidden = view.dockApps !== null;
  page.excludedApps.replaceChildren(
    ...exclusionRows(view.dockApps, view.settings.excluded).map((row) =>
      exclusionItem(row, (excluded) =>
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
