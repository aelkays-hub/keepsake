// Tools → Help: how-to topics and the keyboard shortcuts. Pure data and a
// search over it (unit-tested). Each topic says where things are in the app;
// keep it in step with docs/SPEC.md.

export interface HelpTopic {
  id: string;
  title: string;
  steps: string[];
}

export const HELP_TOPICS: HelpTopic[] = [
  {
    id: "import-card",
    title: "Import a camera card",
    steps: [
      "Insert the card (or connect the camera over USB). The import window opens and shows the items not yet in your library, grouped by day.",
      "Pick the items, a destination (or a preset) and the file-name template. Turn on a second destination to keep two copies.",
      "Click Import. Every copy is read back and checked. The card is reported safe to format only after two checked copies.",
      "Nothing on the card is changed or deleted.",
    ],
  },
  {
    id: "auto-import",
    title: "Import every card automatically",
    steps: [
      "Open Settings → When a card is inserted and choose Copy new photos and videos automatically.",
      "Choose where to copy to and the folder layout (for example year › month › day).",
      "The import window then starts by itself when a card is inserted, and closes 5 seconds after a good import (Keep open stops that).",
    ],
  },
  {
    id: "add-folder",
    title: "Add photos already on your computer or NAS",
    steps: [
      "Open Tools → Folders and, under Add a folder, pick the folder.",
      "It is catalogued where it is: nothing is copied, moved or changed, and nothing is written into the folder.",
      "Its items appear while it is read. Right-click a folder in the sidebar's folder tree → Scan again to pick up new, changed or removed files.",
    ],
  },
  {
    id: "browse",
    title: "Browse by date, folder, aircraft or face",
    steps: [
      "Use the four buttons above the sidebar tree: By date, By folder, By aircraft, By face.",
      "Click a line to open its photos; click the arrow only to open or close it. With the keyboard, → opens or goes in and ← closes or goes up.",
      "Switch the main view with Ctrl+1 (calendar), Ctrl+2 (grid) and Ctrl+3 (details).",
    ],
  },
  {
    id: "search",
    title: "Search and filter",
    steps: [
      "Press Ctrl+F (⌘F on a Mac) or click the search bar above the grid.",
      "Type free text, or filter by date range, camera, lens, type (photo, video, RAW) and minimum rating.",
    ],
  },
  {
    id: "rate",
    title: "Rate, label, comment and add keywords",
    steps: [
      "Select one or more items. Press 1 to 5 for stars (0 clears), and 6 to 9 for the Red, Yellow, Green and Blue labels.",
      "Use the inspector (Ctrl+I shows or hides it) for comments and keywords. Every edit applies to the whole selection.",
      "Edits are written to XMP sidecar files; the original file is never changed.",
    ],
  },
  {
    id: "view",
    title: "View photos and videos",
    steps: [
      "Double-click an item or press Enter. Esc closes the viewer and F switches to full screen.",
      "Zoom with + and −, the mouse wheel or the zoom bar; Z switches between Fit and 100%. R switches a RAW + JPEG pair between its two files. I shows the information panel.",
      "Videos: Space plays, , and . step one frame, J and L jump one second.",
    ],
  },
  {
    id: "time-shift",
    title: "Fix the time on photos from a camera with the wrong clock",
    steps: [
      "Select the photos and press Ctrl+T.",
      "Pick a reference photo, enter the time it really was, and check the offset on the preview.",
      "The corrected time is written to the XMP sidecars, never to the original files.",
    ],
  },
  {
    id: "open-in",
    title: "Open an item in another program",
    steps: [
      "Press Ctrl+O to open it in the default program for its type, or right-click → Open in for the others.",
      "Choose the programs for each file type in Settings → Open in.",
      "Right-click → Show in Explorer (Finder on a Mac) opens the folder with the file selected.",
    ],
  },
  {
    id: "backup",
    title: "Back up and check your library",
    steps: [
      "Open Tools → Backup, choose a destination and start the backup. Only what is new is copied, and every copy is checked.",
      "Click Verify now on a backup at any time to re-read every file and compare it with the catalog. Missing, damaged or changed files are listed.",
    ],
  },
  {
    id: "duplicates",
    title: "Find duplicates",
    steps: [
      "With On-device AI turned on, right-click a year, month or day in the sidebar → Find duplicates, or open Tools → Duplicates for every day already analysed.",
      "In each group of near-identical shots, pick the one to keep (K). The others are marked not kept and can be hidden with the toolbar's Hide not kept.",
      "Nothing is moved or deleted.",
    ],
  },
  {
    id: "ai",
    title: "Tag photos and recognise faces on this computer",
    steps: [
      "Turn on Settings → On-device AI to tag photos, and Settings → Recognise faces to find faces. It runs on this computer, in the background.",
      "Name a person from right-click → Name and find…; their photos then appear under By face.",
    ],
  },
  {
    id: "aircraft",
    title: "Read aircraft registrations, types and airlines",
    steps: [
      "Set up Settings → Read aircraft. Then right-click photos (or a folder or dates in the sidebar) → Read aircraft.",
      "Review what was read and accept it; the keywords then appear in By aircraft and in Tools → Report → Aircraft.",
    ],
  },
  {
    id: "flickr",
    title: "Upload to Flickr",
    steps: [
      "Sign in to Flickr in Settings → Flickr.",
      "Select photos, right-click → Upload to Flickr…, and choose who can see them.",
    ],
  },
  {
    id: "report",
    title: "See how much your library holds",
    steps: [
      "Open Tools → Report for photos, videos and disk space by year, month, day, camera, lens, file type or drive.",
      "Click a row's name to open its items. Export to CSV… saves every table to one file.",
    ],
  },
];

export interface Shortcut {
  keys: string;
  what: string;
}

/** Ctrl on Windows; ⌘ on a Mac. */
export const SHORTCUTS: Shortcut[] = [
  { keys: "Ctrl+1 / Ctrl+2 / Ctrl+3", what: "Calendar, grid, details" },
  { keys: "Ctrl+F", what: "Search" },
  { keys: "Ctrl+A", what: "Select all" },
  { keys: "Ctrl+I", what: "Show or hide the inspector" },
  { keys: "Ctrl+T", what: "Fix the time of the selection" },
  { keys: "Ctrl+O", what: "Open in the default program" },
  { keys: "Ctrl+,", what: "Settings" },
  { keys: "F6", what: "Move between the sidebar, the main area and the inspector" },
  { keys: "Arrows, Page Up/Down, Home, End", what: "Move; with Shift, extend the selection" },
  { keys: "Enter / Backspace", what: "Open / go up a level" },
  { keys: "Esc", what: "Clear the selection, or close the viewer" },
  { keys: "0–5", what: "Stars (0 clears)" },
  { keys: "6 / 7 / 8 / 9", what: "Red, Yellow, Green, Blue label" },
  { keys: "+ / −", what: "Bigger or smaller thumbnails; zoom in the viewer" },
  { keys: "Shift+F10 or the Menu key", what: "The right-click menu" },
];

/** The topics whose title or steps contain every word of `find` (any case). */
export function findTopics(topics: HelpTopic[], find: string): HelpTopic[] {
  const words = find.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return topics;
  return topics.filter((t) => {
    const text = `${t.title} ${t.steps.join(" ")}`.toLowerCase();
    return words.every((w) => text.includes(w));
  });
}
