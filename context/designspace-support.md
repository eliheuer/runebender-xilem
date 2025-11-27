# Designspace File Support Implementation Plan

## Overview

Designspace files (`.designspace`) tie multiple UFO masters together for variable font creation. This feature enables Runebender to:
- Load designspace files and all associated UFO sources
- Switch between masters for editing via a toolbar
- Save changes back to all modified UFOs

## Reference Materials

- [Variable Fonts (Wikipedia)](https://en.wikipedia.org/wiki/Variable_font)
- [RoboFont: Creating Variable Fonts](https://robofont.com/documentation/tutorials/creating-variable-fonts/)
- [Glyphs App: Creating Variable Fonts](https://glyphsapp.com/learn/creating-a-variable-font)
- [Fontra Source (GitHub)](https://github.com/googlefonts/fontra) - Reference implementation
- [norad designspace module](https://docs.rs/norad/latest/norad/designspace/) - Rust parser

## Sample Designspace Structure

From `VirtuaGrotesk.designspace`:
```xml
<?xml version='1.0' encoding='UTF-8'?>
<designspace format="5.0">
  <axes>
    <axis tag="wght" name="Weight" minimum="400" maximum="700" default="400"/>
  </axes>
  <sources>
    <source filename="VirtuaGrotesk-Regular.ufo" name="Virtua Grotesk Regular" ...>
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
    <source filename="VirtuaGrotesk-Bold.ufo" name="Virtua Grotesk Bold" ...>
      <location><dimension name="Weight" xvalue="700"/></location>
    </source>
  </sources>
  <instances>...</instances>
</designspace>
```

## Architecture Decision: norad's designspace module

The `norad` crate (already a dependency) provides designspace parsing via:
- `DesignSpaceDocument` - Main document struct
- `Source` - References to UFO files with locations
- `Axis` - Design axes (wght, wdth, etc.)
- `Instance` - Named font instances

This avoids adding new dependencies.

---

## Implementation Phases

### Phase 1: Data Model & Designspace Loading
- [ ] Create `DesignspaceProject` struct to hold multiple masters
- [ ] Add `Axis` struct for design axes
- [ ] Add `Master` struct (workspace + location + metadata)
- [ ] Parse `.designspace` files using norad
- [ ] Load all referenced UFO sources
- [ ] Handle relative paths for UFO files
- [ ] Track active master index

**Key Files:**
- `src/designspace.rs` (new)
- `src/workspace.rs` (extend or wrap)
- `src/data.rs` (add designspace support to AppState)

### Phase 2: AppState Integration
- [ ] Add `designspace: Option<DesignspaceProject>` to AppState
- [ ] Modify `open_font_dialog` to accept `.designspace` files
- [ ] Add `load_designspace()` method
- [ ] Update workspace access to go through active master
- [ ] Create `active_workspace()` helper that returns current master's workspace

**Changes to existing code:**
- File dialog filter: add `*.designspace`
- Route workspace access through designspace when present

### Phase 3: Master Toolbar UI
- [ ] Create `MasterToolbar` component
- [ ] Generate dynamic icons using "n" glyph (U+006E) from each master
- [ ] Show master name on hover
- [ ] Highlight active master
- [ ] Handle click to switch masters
- [ ] Position below workspace toolbar (top-right)

**UI Design:**
```
┌─────────────────────────────────────────────────────────┐
│ [Edit Tools]            [Workspace Tools] [Master: n n] │
│                                           Regular Bold  │
└─────────────────────────────────────────────────────────┘
```

### Phase 4: Master Switching
- [ ] Sync current edits before switching
- [ ] Load new master's glyph into editor
- [ ] Preserve selection state when possible
- [ ] Update text buffer for new master
- [ ] Handle missing glyphs (show placeholder)

**Switching flow:**
1. User clicks master button
2. Sync current session to workspace
3. Set new active master index
4. Reload current glyph from new master
5. Update all UI elements

### Phase 5: Saving
- [ ] Add `save_designspace()` method
- [ ] Save all modified UFO workspaces
- [ ] Preserve designspace XML structure
- [ ] Track which masters have unsaved changes
- [ ] Update Cmd+S handler

**Save behavior:**
- Sync current editor session
- For each master: if modified, save UFO
- Save designspace file (preserve structure)

---

## Data Structures

### DesignspaceProject
```rust
/// A designspace project containing multiple font masters
pub struct DesignspaceProject {
    /// Path to the .designspace file
    pub path: PathBuf,

    /// Design axes (wght, wdth, etc.)
    pub axes: Vec<DesignAxis>,

    /// Font masters (one workspace per source)
    pub masters: Vec<Master>,

    /// Index of the currently active master
    pub active_master: usize,

    /// Named instances (for reference, not editable)
    pub instances: Vec<Instance>,

    /// Original designspace document (for round-tripping)
    designspace_doc: norad::designspace::DesignSpaceDocument,
}

/// A design axis
pub struct DesignAxis {
    pub tag: String,      // e.g., "wght"
    pub name: String,     // e.g., "Weight"
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
}

/// A font master (source in designspace terms)
pub struct Master {
    /// Display name
    pub name: String,

    /// Style name (e.g., "Regular", "Bold")
    pub style_name: String,

    /// Location in design space
    pub location: HashMap<String, f64>,

    /// The loaded workspace
    pub workspace: Workspace,

    /// Whether this master has unsaved changes
    pub modified: bool,
}
```

### AppState Changes
```rust
pub struct AppState {
    // Existing fields...

    /// Single UFO workspace (legacy, for non-designspace files)
    pub workspace: Option<Arc<RwLock<Workspace>>>,

    /// Designspace project (when .designspace is loaded)
    pub designspace: Option<DesignspaceProject>,

    // Helper method
    pub fn active_workspace(&self) -> Option<Arc<RwLock<Workspace>>> {
        if let Some(ds) = &self.designspace {
            Some(ds.active_workspace())
        } else {
            self.workspace.clone()
        }
    }
}
```

---

## UI Components

### Master Toolbar

**Location:** Below workspace toolbar, top-right corner

**Button design:**
- Each button shows the "n" glyph rendered from that master
- Button size: 36x36px (smaller than main toolbar)
- Active master: highlighted background
- Inactive: muted colors

**Implementation approach:**
- Pre-render "n" glyph to BezPath for each master
- Use existing `glyph_view` widget pattern
- Add hover tooltip with master name

---

## File Dialog Changes

Current:
```rust
file_dialog.add_filter("UFO Font", &["ufo"]);
```

New:
```rust
file_dialog.add_filter("UFO Font", &["ufo"]);
file_dialog.add_filter("Designspace", &["designspace"]);
file_dialog.add_filter("All Font Sources", &["ufo", "designspace"]);
```

---

## Test File

Use for development and testing:
```
~/GH/repos/virtua-grotesk/sources/VirtuaGrotesk.designspace
```

Contains:
- 2 masters: Regular (wght=400), Bold (wght=700)
- 1 axis: Weight
- 4 instances

---

## Implementation Checklist

### Phase 1: Data Model & Loading
- [ ] Add `norad::designspace` imports
- [ ] Create `src/designspace.rs` with data structures
- [ ] Implement `DesignspaceProject::load(path)`
- [ ] Handle relative UFO paths
- [ ] Load all source UFOs into workspaces
- [ ] Unit tests for loading

### Phase 2: AppState Integration
- [ ] Add `designspace` field to AppState
- [ ] Add `load_designspace()` method
- [ ] Update file dialog to accept `.designspace`
- [ ] Create `active_workspace()` accessor
- [ ] Update all workspace references to use accessor
- [ ] Test with sample designspace

### Phase 3: Master Toolbar
- [ ] Create `MasterToolbar` component
- [ ] Add to editor view layout
- [ ] Generate "n" glyph icons dynamically
- [ ] Style active/inactive states
- [ ] Add click handlers
- [ ] Test appearance with 2+ masters

### Phase 4: Master Switching
- [ ] Implement `switch_master(index)` method
- [ ] Sync editor before switch
- [ ] Reload current glyph from new master
- [ ] Preserve viewport/zoom if possible
- [ ] Handle glyph not in new master
- [ ] Test editing across masters

### Phase 5: Saving
- [ ] Implement `save_designspace()` method
- [ ] Track modified state per master
- [ ] Save only modified UFOs
- [ ] Preserve designspace structure
- [ ] Wire up Cmd+S for designspace
- [ ] Test save/reload cycle

---

## Future Enhancements (Not in V1)

- **Interpolation preview**: Show interpolated glyph in editor
- **Cross-master comparison**: Side-by-side view
- **Compatibility checking**: Warn about point count mismatches
- **Instance generation**: Export instances from designspace
- **Axis sliders**: Interactive axis exploration
- **New master creation**: Add sources to designspace

---

## Notes from Fontra Research

Fontra's architecture (for reference):
- Uses async backends for non-blocking file I/O
- Supports "sparse" sources (layers within UFOs)
- Tracks component dependencies for cascading updates
- Uses change queuing for safe concurrent writes
- Monitors files for external changes

For V1, we use a simpler synchronous approach with the existing Workspace architecture.
