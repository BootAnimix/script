<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    Accordion,
    AccordionItem,
    Button,
    InlineNotification,
    Loading,
    Modal,
    NumberInput,
    Search,
    Select,
    SelectItem,
    Slider,
    Tab,
    TabContent,
    Tabs,
    TextInput,
    Toggle,
  } from 'carbon-components-svelte';
  import {
    Add,
    CheckmarkFilled,
    ChevronLeft,
    ChevronRight,
    Copy,
    DocumentExport,
    ErrorFilled,
    FolderOpen,
    Image as ImageIcon,
    Information,
    Pause,
    Play,
    Redo,
    Save,
    TrashCan,
    Undo,
    WarningAlt,
  } from 'carbon-icons-svelte';
  import { StudioBridge } from './lib/bridge';
  import type {
    Diagnostic,
    PartDeclaration,
    RepeatCount,
    RgbColor,
    ScheduledFrame,
    WorkspaceSnapshot,
  } from './lib/types';

  const bridge = new StudioBridge();

  /** Fixed render height of the preview canvas; width follows the animation aspect ratio. */
  const CANVAS_HEIGHT = 420;

  let snapshot = $state<WorkspaceSnapshot | null>(null);
  let selectedPartIdx = $state(0);
  let selectedFrameIdx = $state(0);
  let currentTick = $state(0);
  let isPlaying = $state(false);
  let playbackSpeed = $state(1);
  let canvasZoom = $state(1);
  let previewMode = $state<'full' | 'selected_part'>('full');
  let assetFilter = $state('');
  let showExportModal = $state(false);
  let exportTargetKind = $state<'bootanimation' | 'magisk'>('bootanimation');
  let exportDestination = $state('dist/bootanimation.zip');
  let exporting = $state(false);
  let buildResult = $state<{ kind: 'success' | 'error'; title: string; text: string } | null>(null);
  let statusBanner = $state('No project open. Open a project folder to load parts and frames.');
  let newPartName = $state('');
  let dockTab = $state(0);

  let canvasEl = $state<HTMLCanvasElement | undefined>();
  let stageEl = $state<HTMLDivElement | undefined>();
  let fitScale = $state(1);
  let timerId: ReturnType<typeof setInterval> | null = null;

  let activePartDecl = $derived(snapshot?.manifest.descriptor.parts[selectedPartIdx]);
  let activeProjectPart = $derived(snapshot?.manifest.parts[selectedPartIdx]);
  let activeFrame = $derived<ScheduledFrame | undefined>(snapshot?.schedule.frames[currentTick]);
  let totalFrames = $derived(snapshot?.validation.performance.total_frames ?? 0);
  let hasParts = $derived((snapshot?.manifest.parts.length ?? 0) > 0);
  let diagnostics = $derived(snapshot?.validation.diagnostics ?? []);
  let blockingCount = $derived(diagnostics.filter((d) => d.blocks_export || d.severity === 'error').length);
  let warningCount = $derived(diagnostics.filter((d) => d.severity === 'warning' && !d.blocks_export).length);

  let canvasWidth = $derived.by(() => {
    const desc = snapshot?.manifest.descriptor;
    if (!desc) return Math.round(CANVAS_HEIGHT * 0.45);
    return Math.max(110, Math.round(CANVAS_HEIGHT * (desc.width / Math.max(1, desc.height))));
  });

  let filteredFrames = $derived.by(() => {
    const frames = activeProjectPart?.frames ?? [];
    const query = assetFilter.trim().toLowerCase();
    return frames
      .map((frame, index) => ({ frame, index }))
      .filter(({ frame }) => !query || frame.relative_path.toLowerCase().includes(query));
  });

  let partFrameCount = $derived(activeProjectPart?.frames.length ?? 0);
  let scheduleLength = $derived(snapshot?.schedule.frames.length ?? 0);
  let displayWidth = $derived(Math.round(canvasWidth * fitScale * canvasZoom));
  let displayHeight = $derived(Math.round(CANVAS_HEIGHT * fitScale * canvasZoom));

  onMount(async () => {
    await refresh();
  });

  onDestroy(() => {
    stopPlayback();
  });

  // The canvas is an immediate-mode drawing surface, so redraw it whenever the
  // timeline position, geometry or loaded project changes.
  $effect(() => {
    void snapshot;
    void currentTick;
    void canvasWidth;
    void canvasEl;
    drawCanvasFrame();
  });

  // The device preview keeps its real aspect ratio but has to remain visible
  // in short windows (the Tauri window can be as small as 980×640), so the
  // stage reports its available box and the canvas is scaled to fit it.
  $effect(() => {
    const element = stageEl;
    if (!element) return;

    const measure = () => {
      const note = element.querySelector<HTMLElement>('.stage-note');
      const noteBlock = note ? note.offsetHeight + 10 : 0;
      const availableHeight = Math.max(100, element.clientHeight - noteBlock - 34);
      const availableWidth = Math.max(100, element.clientWidth - 58);
      fitScale = Math.min(
        1,
        availableHeight / CANVAS_HEIGHT,
        availableWidth / Math.max(1, canvasWidth),
      );
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  });

  async function refresh(partFilter: number | null = null) {
    const filter = previewMode === 'selected_part' ? selectedPartIdx : partFilter;
    let next = await bridge.getSnapshot(filter);

    // A delete or an undo can leave the selection past the end of the project.
    let resynced = false;
    if (selectedPartIdx >= next.manifest.parts.length) {
      selectedPartIdx = Math.max(0, next.manifest.parts.length - 1);
      resynced = true;
    }
    if (selectedFrameIdx >= (next.manifest.parts[selectedPartIdx]?.frames.length ?? 0)) {
      selectedFrameIdx = 0;
    }

    // In "selected part" scope the schedule was built for the stale index, so
    // rebuild it once with the corrected selection.
    if (previewMode === 'selected_part' && (resynced || filter !== selectedPartIdx)) {
      next = await bridge.getSnapshot(selectedPartIdx);
    }

    snapshot = next;
    if (next.schedule.frames.length === 0) {
      currentTick = 0;
    } else if (currentTick >= next.schedule.frames.length) {
      currentTick = next.schedule.frames.length - 1;
    }
  }

  function startPlayback() {
    if (!snapshot || snapshot.schedule.frames.length === 0) return;
    stopPlayback();
    isPlaying = true;
    const fps = Math.max(1, snapshot.schedule.fps * playbackSpeed);
    const intervalMs = Math.max(12, Math.round(1000 / fps));
    timerId = setInterval(() => {
      if (!snapshot || snapshot.schedule.frames.length === 0) return;
      currentTick = (currentTick + 1) % snapshot.schedule.frames.length;
    }, intervalMs);
  }

  function stopPlayback() {
    isPlaying = false;
    if (timerId !== null) {
      clearInterval(timerId);
      timerId = null;
    }
  }

  function togglePlayback() {
    if (isPlaying) stopPlayback();
    else startPlayback();
  }

  function stepFrame(delta: number) {
    stopPlayback();
    if (scheduleLength === 0) return;
    currentTick = (currentTick + delta + scheduleLength) % scheduleLength;
  }

  async function selectPart(idx: number) {
    selectedPartIdx = idx;
    selectedFrameIdx = 0;
    if (previewMode === 'selected_part') {
      await refresh(idx);
      currentTick = 0;
      return;
    }
    const firstTick = snapshot?.schedule.frames.findIndex((frame) => frame.part_index === idx);
    if (firstTick !== undefined && firstTick >= 0) {
      currentTick = firstTick;
    }
  }

  function selectFrame(idx: number) {
    selectedFrameIdx = idx;
    const scheduled = snapshot?.schedule.frames.findIndex(
      (frame) => frame.part_index === selectedPartIdx && frame.frame_index_in_part === idx,
    );
    if (scheduled !== undefined && scheduled >= 0) {
      currentTick = scheduled;
    }
  }

  function drawCanvasFrame() {
    const canvas = canvasEl;
    if (!canvas || !snapshot) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const w = canvas.width;
    const h = canvas.height;
    const schedule = snapshot.schedule;
    const frame = schedule.frames[currentTick];

    ctx.clearRect(0, 0, w, h);

    const bg = frame?.background_color ?? { r: 8, g: 8, b: 8 };
    ctx.fillStyle = `rgb(${bg.r}, ${bg.g}, ${bg.b})`;
    ctx.fillRect(0, 0, w, h);

    if (!frame) return;

    const cx = w / 2;
    const cy = h / 2;
    const radius = Math.min(w, h) * 0.27;
    const gradient = ctx.createLinearGradient(cx - radius, cy - radius, cx + radius, cy + radius);
    gradient.addColorStop(0, '#fa4d56');
    gradient.addColorStop(0.36, '#ee5396');
    gradient.addColorStop(0.68, '#a56eff');
    gradient.addColorStop(1, '#78a9ff');

    ctx.save();
    ctx.globalAlpha = frame.fade_opacity;

    // Trim bounding box reported by the frame assets.
    if (frame.trim) {
      const sx = w / Math.max(1, schedule.width);
      const sy = h / Math.max(1, schedule.height);
      ctx.strokeStyle = 'rgba(120, 169, 255, 0.55)';
      ctx.lineWidth = 1;
      ctx.setLineDash([3, 3]);
      ctx.strokeRect(
        frame.trim.x * sx + 0.5,
        frame.trim.y * sy + 0.5,
        frame.trim.width * sx,
        frame.trim.height * sy,
      );
      ctx.setLineDash([]);
    }

    // The brand mark is the shape language of the animation preview.
    const phase = (currentTick * 0.045) % (Math.PI * 2);
    ctx.strokeStyle = gradient;
    ctx.lineWidth = Math.max(7, radius * 0.34);
    ctx.beginPath();
    ctx.arc(cx, cy, radius, phase, phase + Math.PI * 1.55);
    ctx.stroke();

    if (frame.is_pause_hold) {
      ctx.fillStyle = 'rgba(244, 244, 244, 0.9)';
      ctx.fillRect(cx - 7, cy - 12, 4, 24);
      ctx.fillRect(cx + 3, cy - 12, 4, 24);
    }

    ctx.fillStyle = 'rgba(244, 244, 244, 0.72)';
    ctx.font = '500 10px "IBM Plex Mono", monospace';
    ctx.fillText(frame.part_name, 12, h - 30);
    ctx.fillStyle = 'rgba(244, 244, 244, 0.48)';
    ctx.fillText(
      `F${String(frame.frame_index_in_part).padStart(4, '0')}  ·  ${frame.is_pause_hold ? 'PAUSE' : 'PLAY'}`,
      12,
      h - 16,
    );

    if (snapshot.manifest.descriptor.show_progress) {
      const progress = (currentTick + 1) / Math.max(1, schedule.frames.length);
      ctx.fillStyle = 'rgba(244, 244, 244, 0.16)';
      ctx.fillRect(12, h - 8, w - 24, 2);
      ctx.fillStyle = gradient;
      ctx.fillRect(12, h - 8, (w - 24) * progress, 2);
    }

    ctx.restore();
  }

  async function switchScope(value: string) {
    previewMode = value === 'selected_part' ? 'selected_part' : 'full';
    stopPlayback();
    await refresh();
  }

  async function changeSpeed(value: string) {
    playbackSpeed = Number.parseFloat(value) || 1;
    if (isPlaying) startPlayback();
  }

  async function handleUndo() {
    stopPlayback();
    await bridge.undo(previewMode === 'selected_part' ? selectedPartIdx : null);
    await refresh();
    statusBanner = 'Undid the last edit.';
  }

  async function handleRedo() {
    stopPlayback();
    await bridge.redo(previewMode === 'selected_part' ? selectedPartIdx : null);
    await refresh();
    statusBanner = 'Redid the last edit.';
  }

  async function handleSaveProject() {
    snapshot = await bridge.markSaved();
    statusBanner = `Saved ${snapshot.manifest.name}/project.bootanimix.json`;
  }

  async function handleOpenProject() {
    if (!bridge.isDesktop) {
      statusBanner = 'Opening a project from disk requires the desktop app.';
      return;
    }
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const picked = await open({ directory: true, multiple: false, title: 'Open BootAnimix project' });
      if (typeof picked !== 'string') return;
      snapshot = await bridge.openProject(picked);
      selectedPartIdx = 0;
      selectedFrameIdx = 0;
      currentTick = 0;
      statusBanner = `Opened ${picked}`;
    } catch (err) {
      statusBanner = `Open failed: ${err instanceof Error ? err.message : String(err)}`;
    }
  }

  async function handleBrowseDestination() {
    if (!bridge.isDesktop) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const picked = await save({
        title: 'Export boot animation archive',
        defaultPath: exportDestination,
        filters: [{ name: 'ZIP archive', extensions: ['zip'] }],
      });
      if (typeof picked === 'string') {
        exportDestination = picked;
      }
    } catch (err) {
      statusBanner = `Browse failed: ${err instanceof Error ? err.message : String(err)}`;
    }
  }

  function applySnapshot(next: WorkspaceSnapshot, message?: string) {
    snapshot = next;
    if (message) statusBanner = message;
  }

  async function updateHeaderNumber(field: 'width' | 'height' | 'fps', raw: number) {
    const value = Math.max(1, Math.round(raw || 1));
    const next = await bridge.commitMutation(
      (manifest) => {
        manifest.descriptor[field] = value;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    applySnapshot(next, `Header ${field} set to ${value}.`);
  }

  async function toggleProgressFlag(checked: boolean) {
    const next = await bridge.commitMutation(
      (manifest) => {
        manifest.descriptor.show_progress = checked;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    applySnapshot(next, `Boot progress indicator ${checked ? 'enabled' : 'disabled'}.`);
  }

  async function updatePartMode(modeKey: string) {
    const next = await bridge.commitMutation(
      (manifest) => {
        const part = manifest.descriptor.parts[selectedPartIdx];
        if (!part) return;
        if (modeKey === 'p') part.mode = { mode: 'play_interruptible' };
        else if (modeKey === 'c') part.mode = { mode: 'play_to_completion' };
        else if (modeKey === 'f') part.mode = { mode: 'fade_out', fade_frames: 10 };
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    applySnapshot(next, `Playback mode for ${activePartDecl?.path ?? 'part'} updated.`);
  }

  async function updatePartRepeat(rawCount: number) {
    const num = Math.max(0, Math.round(rawCount || 0));
    const repeat: RepeatCount = num === 0 ? { kind: 'infinite' } : { kind: 'finite', count: num };
    const next = await bridge.commitMutation(
      (manifest) => {
        const part = manifest.descriptor.parts[selectedPartIdx];
        if (part) part.repeat = repeat;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    applySnapshot(next, num === 0 ? 'Part now loops until boot completes.' : `Part repeats ${num}×.`);
  }

  async function updatePartPause(rawPause: number) {
    const pause = Math.max(0, Math.round(rawPause || 0));
    const next = await bridge.commitMutation(
      (manifest) => {
        const part = manifest.descriptor.parts[selectedPartIdx];
        if (part) part.pause_frames = pause;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    applySnapshot(next, `Pause hold set to ${pause} frame${pause === 1 ? '' : 's'}.`);
  }

  async function updatePartBgHex(hex: string) {
    const clean = hex.replace('#', '').trim();
    if (clean.length !== 6) return;
    const r = Number.parseInt(clean.slice(0, 2), 16);
    const g = Number.parseInt(clean.slice(2, 4), 16);
    const b = Number.parseInt(clean.slice(4, 6), 16);
    if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) return;
    const color: RgbColor = { r, g, b };
    const next = await bridge.commitMutation(
      (manifest) => {
        const part = manifest.descriptor.parts[selectedPartIdx];
        if (part) part.background_color = color;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    applySnapshot(next, `Background colour set to #${clean.toUpperCase()}.`);
  }

  async function addNewPart() {
    if (!snapshot) return;
    const name = (newPartName.trim() || `part${snapshot.manifest.parts.length}`).replace(
      /[^a-zA-Z0-9_-]/g,
      '',
    );
    if (!name || snapshot.manifest.parts.some((p) => p.name === name)) {
      statusBanner = `Part '${name}' already exists or is invalid.`;
      return;
    }
    const next = await bridge.commitMutation((manifest) => {
      const declaration: PartDeclaration = {
        mode: { mode: 'play_interruptible' },
        repeat: { kind: 'finite', count: 1 },
        pause_frames: 0,
        path: name,
        background_color: { r: 0, g: 0, b: 0 },
        clock: null,
      };
      manifest.descriptor.parts.push(declaration);
      manifest.parts.push({
        name,
        frames: [{ relative_path: `assets/${name}/00000.png`, trim: null }],
        audio_relative_path: null,
      });
    });
    newPartName = '';
    applySnapshot(next);
    selectedPartIdx = next.manifest.parts.length - 1;
    selectedFrameIdx = 0;
    statusBanner = `Added part '${name}'.`;
  }

  async function deleteSelectedPart() {
    if (!snapshot || snapshot.manifest.parts.length <= 1) {
      statusBanner = 'Cannot delete the last remaining part.';
      return;
    }
    const removedName = snapshot.manifest.parts[selectedPartIdx]?.name;
    const next = await bridge.commitMutation((manifest) => {
      manifest.descriptor.parts.splice(selectedPartIdx, 1);
      manifest.parts.splice(selectedPartIdx, 1);
    }, previewMode === 'selected_part' ? Math.max(0, selectedPartIdx - 1) : null);
    selectedPartIdx = Math.max(0, selectedPartIdx - 1);
    selectedFrameIdx = 0;
    applySnapshot(next, `Removed part '${removedName}'.`);
  }

  async function moveSelectedFrame(delta: number) {
    if (!snapshot) return;
    const part = snapshot.manifest.parts[selectedPartIdx];
    if (!part) return;
    const target = selectedFrameIdx + delta;
    if (target < 0 || target >= part.frames.length) return;
    const next = await bridge.commitMutation((manifest) => {
      const frames = manifest.parts[selectedPartIdx].frames;
      const [item] = frames.splice(selectedFrameIdx, 1);
      frames.splice(target, 0, item);
    });
    selectedFrameIdx = target;
    applySnapshot(next, `Moved frame to position ${target + 1} in ${part.name}.`);
  }

  async function duplicateSelectedFrame() {
    if (!snapshot) return;
    const part = snapshot.manifest.parts[selectedPartIdx];
    if (!part || !part.frames[selectedFrameIdx]) return;
    const next = await bridge.commitMutation((manifest) => {
      const frames = manifest.parts[selectedPartIdx].frames;
      const copy = structuredClone(frames[selectedFrameIdx]);
      frames.splice(selectedFrameIdx + 1, 0, copy);
    });
    selectedFrameIdx += 1;
    applySnapshot(next, `Duplicated frame in ${part.name}.`);
  }

  async function removeSelectedFrame() {
    if (!snapshot) return;
    const part = snapshot.manifest.parts[selectedPartIdx];
    if (!part || part.frames.length <= 1) {
      statusBanner = 'Cannot remove the last remaining frame in a part.';
      return;
    }
    const next = await bridge.commitMutation((manifest) => {
      manifest.parts[selectedPartIdx].frames.splice(selectedFrameIdx, 1);
    });
    selectedFrameIdx = Math.max(0, selectedFrameIdx - 1);
    applySnapshot(next, `Removed frame from ${part.name}.`);
  }

  async function commitMagiskId(value: string) {
    const next = await bridge.commitMutation((manifest) => {
      manifest.magisk.module_id = value;
    });
    applySnapshot(next, 'Magisk module ID updated.');
  }

  async function commitMagiskTarget(value: string) {
    const next = await bridge.commitMutation((manifest) => {
      manifest.magisk.target_path = value;
    });
    applySnapshot(next, `Overlay target set to /${value}.`);
  }

  async function toggleTransform(key: 'lossless_optimize_png' | 'generate_trim_txt', checked: boolean) {
    const next = await bridge.commitMutation((manifest) => {
      manifest.transforms[key] = checked;
    });
    applySnapshot(next);
  }

  function focusDiagnosticTarget(diag: Diagnostic) {
    if (!snapshot || !diag.file_or_part) return;
    const idx = snapshot.manifest.parts.findIndex(
      (part) => diag.file_or_part === part.name || diag.file_or_part?.startsWith(`${part.name}/`),
    );
    if (idx >= 0) {
      selectedPartIdx = idx;
      selectedFrameIdx = 0;
      statusBanner = `Focused part '${snapshot.manifest.parts[idx].name}' from ${diag.code}.`;
    } else {
      statusBanner = `Focused '${diag.file_or_part}' (${diag.code}).`;
    }
  }

  function formatRgbHex(color: RgbColor | null | undefined): string {
    if (!color) return '#000000';
    const hex = (n: number) => n.toString(16).padStart(2, '0').toUpperCase();
    return `#${hex(color.r)}${hex(color.g)}${hex(color.b)}`;
  }

  function openExport() {
    buildResult = null;
    showExportModal = true;
  }

  function setExportKind(kind: string) {
    exportTargetKind = kind === 'magisk' ? 'magisk' : 'bootanimation';
    exportDestination =
      exportTargetKind === 'bootanimation'
        ? 'dist/bootanimation.zip'
        : `dist/${snapshot?.manifest.magisk.module_id ?? 'bootanimix_module'}.zip`;
  }

  async function executeReviewedExport() {
    if (!snapshot) return;
    if (snapshot.validation.outcome === 'invalid') {
      buildResult = {
        kind: 'error',
        title: 'Export blocked',
        text: 'Resolve the blocking diagnostics before writing an archive.',
      };
      return;
    }
    if (!bridge.isDesktop) {
      buildResult = {
        kind: 'error',
        title: 'Desktop app required',
        text: 'The browser preview has no filesystem access. Run the Tauri shell to write a real archive.',
      };
      return;
    }

    exporting = true;
    buildResult = null;
    try {
      if (exportTargetKind === 'bootanimation') {
        const report = await bridge.buildArchive(exportDestination);
        buildResult = {
          kind: 'success',
          title: 'Archive written',
          text: `${report.output_path} · ${report.total_frames} frames across ${snapshot.manifest.parts.length} parts. Compression method ${report.compression_method}, verified after reopen.`,
        };
        statusBanner = `Exported verified ${report.output_path}`;
      } else {
        const report = await bridge.exportMagisk(exportDestination);
        buildResult = {
          kind: 'success',
          title: 'Magisk module written',
          text: `${report.output_path} · id ${report.module_id} · overlay /${report.target_path}. Inner archive verified.`,
        };
        statusBanner = `Exported Magisk module ${report.output_path}`;
      }
      await refresh();
    } catch (err) {
      buildResult = {
        kind: 'error',
        title: 'Export failed',
        text: err instanceof Error ? err.message : String(err),
      };
      statusBanner = 'Export failed. See the build log.';
    } finally {
      exporting = false;
    }
  }
</script>

{#if snapshot}
  <div class="studio">
    <header class="app-header">
      <div class="brand">
        <img class="brand-mark" src="/logo-128.png" alt="" width="26" height="26" />
        <span class="brand-name">BootAnimix</span>
        <span class="brand-product">Studio</span>
        <span class="version mono">3.5.1</span>
      </div>

      <div class="project-id">
        <span class="project-name">{snapshot.manifest.name}</span>
        {#if snapshot.dirty}
          <span class="unsaved">Unsaved</span>
        {/if}
      </div>

      <div class="header-actions">
        <Button kind="ghost" size="small" disabled={!bridge.isDesktop} onclick={handleOpenProject} icon={FolderOpen}>Open</Button>
        <Button kind="ghost" size="small" disabled={!snapshot.project_dir} onclick={handleSaveProject} icon={Save}>Save</Button>
        <span class="divider" aria-hidden="true"></span>
        <Button kind="ghost" size="small" iconDescription="Undo" disabled={!snapshot.can_undo} onclick={handleUndo} icon={Undo}></Button>
        <Button kind="ghost" size="small" iconDescription="Redo" disabled={!snapshot.can_redo} onclick={handleRedo} icon={Redo}></Button>
        <span class="divider" aria-hidden="true"></span>

        <span class="validation-chip {hasParts ? snapshot.validation.outcome : 'empty'}">
          {#if !hasParts}
            <Information size={16} />
            <span>No project</span>
          {:else if snapshot.validation.outcome === 'valid'}
            <CheckmarkFilled size={16} />
            <span>Valid</span>
          {:else if snapshot.validation.outcome === 'valid_with_warnings'}
            <WarningAlt size={16} />
            <span>{warningCount} warning{warningCount === 1 ? '' : 's'}</span>
          {:else}
            <ErrorFilled size={16} />
            <span>{blockingCount} blocking</span>
          {/if}
        </span>

        <Button
          kind="primary"
          size="small"
          disabled={totalFrames === 0}
          onclick={openExport}
          icon={DocumentExport}
        >
          Build &amp; export
        </Button>
      </div>
    </header>

    <div class="workspace">
      <aside class="pane parts-pane">
        <div class="pane-head">
          <h2>Parts</h2>
          <span class="count mono">{snapshot.manifest.parts.length}</span>
          <span class="spacer"></span>
          <Button kind="ghost" size="small" iconDescription="Remove selected part" disabled={snapshot.manifest.parts.length <= 1} onclick={deleteSelectedPart} icon={TrashCan}></Button>
        </div>

        <div class="part-list">
          {#if !hasParts}
            <p class="empty">No parts yet. Open a project folder to load its animation.</p>
          {/if}
          {#each snapshot.manifest.descriptor.parts as part, idx (part.path + idx)}
            {@const frames = snapshot.manifest.parts[idx]?.frames.length ?? 0}
            <button
              type="button"
              class="part-row"
              class:is-selected={idx === selectedPartIdx}
              onclick={() => selectPart(idx)}
              aria-current={idx === selectedPartIdx ? 'true' : undefined}
            >
              <span class="mode-tag mono">
                {part.mode.mode === 'play_interruptible'
                  ? 'p'
                  : part.mode.mode === 'play_to_completion'
                    ? 'c'
                    : part.mode.mode === 'fade_out'
                      ? 'f'
                      : '$'}
              </span>
              <span class="part-label">
                <span class="part-name">{part.path}</span>
                <span class="part-sub mono">
                  {part.repeat.kind === 'infinite' ? '∞ loop' : `${part.repeat.count}×`}
                  {#if part.pause_frames > 0}· {part.pause_frames}f hold{/if}
                </span>
              </span>
              <span class="part-count mono">{frames}f</span>
            </button>
          {/each}
        </div>

        <div class="new-part">
          <TextInput
            size="sm"
            labelText="New part name"
            hideLabel
            placeholder="New part name"
            bind:value={newPartName}
          />
          <Button kind="tertiary" size="small" iconDescription="Add part" onclick={addNewPart} icon={Add}></Button>
        </div>

        <div class="pane-head sub">
          <h2>Frames</h2>
          <span class="count mono">{partFrameCount}</span>
          <span class="spacer"></span>
          <span class="pane-context mono">{activeProjectPart?.name ?? '—'}</span>
        </div>

        <div class="frame-search">
          <Search
            size="sm"
            labelText="Filter frames"
            hideLabel
            placeholder="Filter frames"
            bind:value={assetFilter}
          />
        </div>

        <div class="frame-list">
          {#if !activeProjectPart}
            <p class="empty">No part selected.</p>
          {:else if filteredFrames.length === 0}
            <p class="empty">
              {partFrameCount === 0 ? 'This part has no frames yet.' : `No frames match “${assetFilter}”.`}
            </p>
          {:else}
            {#each filteredFrames as { frame, index } (frame.relative_path + index)}
              <button
                type="button"
                class="frame-row"
                class:is-selected={index === selectedFrameIdx}
                onclick={() => (selectedFrameIdx = index)}
              >
                <span class="frame-index mono">{String(index + 1).padStart(3, '0')}</span>
                <span class="frame-path mono">{frame.relative_path.split('/').pop()}</span>
                {#if frame.trim}
                  <span class="frame-trim mono">{frame.trim.width}×{frame.trim.height}</span>
                {/if}
              </button>
            {/each}
          {/if}
        </div>
      </aside>

      <main class="pane stage-pane">
        <div class="stage-head">
          <h2>Preview</h2>
          <div class="stage-controls">
            <Select
              size="sm"
              inline
              labelText="Scope"
              selected={previewMode}
              on:update={(event) => switchScope(String(event.detail))}
            >
              <SelectItem value="full" text="Whole animation" />
              <SelectItem value="selected_part" text="Selected part" />
            </Select>
            <Select
              size="sm"
              inline
              labelText="Speed"
              selected={String(playbackSpeed)}
              on:update={(event) => changeSpeed(String(event.detail))}
            >
              <SelectItem value="0.25" text="0.25×" />
              <SelectItem value="0.5" text="0.5×" />
              <SelectItem value="1" text="1×" />
              <SelectItem value="2" text="2×" />
            </Select>
            <Select
              size="sm"
              inline
              labelText="Zoom"
              selected={String(canvasZoom)}
              on:update={(event) => (canvasZoom = Number.parseFloat(String(event.detail)) || 1)}
            >
              <SelectItem value="0.75" text="75%" />
              <SelectItem value="1" text="100%" />
              <SelectItem value="1.25" text="125%" />
            </Select>
          </div>
          <span class="stage-meta mono">
            {snapshot.manifest.descriptor.width}×{snapshot.manifest.descriptor.height}
            · {snapshot.manifest.descriptor.fps} fps
          </span>
        </div>

        <div class="stage" bind:this={stageEl}>
          {#if scheduleLength === 0}
            <div class="stage-empty">
              <span class="stage-empty-icon"><ImageIcon size={24} /></span>
              <p class="stage-empty-title">Nothing to preview</p>
              <p class="stage-empty-body">
                {hasParts
                  ? 'This project has no frames yet.'
                  : 'Open a project folder containing project.bootanimix.json to load its parts and frames.'}
              </p>
              {#if bridge.isDesktop}
                <Button kind="tertiary" size="small" icon={FolderOpen} onclick={handleOpenProject}>
                  Open project
                </Button>
              {:else}
                <p class="hint stage-empty-hint">
                  Opening a project from disk requires the desktop app.
                </p>
              {/if}
            </div>
          {:else}
            <div class="device" style="width: {displayWidth + 24}px">
              <canvas
                bind:this={canvasEl}
                width={canvasWidth}
                height={CANVAS_HEIGHT}
                style="width: {displayWidth}px; height: {displayHeight}px"
              ></canvas>
            </div>
            <p class="stage-note">
              <Information size={14} />
              Geometry and timing preview. Frame pixels stay on disk until export.
            </p>
          {/if}
        </div>

        <div class="transport">
          <Button
            kind="ghost"
            size="small"
            icon={ChevronLeft}
            iconDescription="Previous frame"
            disabled={scheduleLength === 0}
            onclick={() => stepFrame(-1)}
          ></Button>
          <Button
            kind="primary"
            size="small"
            icon={isPlaying ? Pause : Play}
            iconDescription={isPlaying ? 'Pause preview' : 'Play preview'}
            disabled={scheduleLength === 0}
            onclick={togglePlayback}
          ></Button>
          <Button
            kind="ghost"
            size="small"
            icon={ChevronRight}
            iconDescription="Next frame"
            disabled={scheduleLength === 0}
            onclick={() => stepFrame(1)}
          ></Button>

          <div class="scrub">
            <Slider
              labelText="Timeline position"
              hideLabel
              hideTextInput
              min={0}
              max={Math.max(0, scheduleLength - 1)}
              bind:value={currentTick}
              on:input={() => stopPlayback()}
            />
          </div>

          <span class="readout mono">
            {(scheduleLength === 0 ? '000' : String(currentTick + 1).padStart(3, '0'))}/{String(
              scheduleLength,
            ).padStart(3, '0')}
            <span class="readout-divider">·</span>
            {activeFrame ? activeFrame.timestamp_seconds.toFixed(2) : '0.00'}s
            {#if snapshot.schedule.is_infinite}<span class="readout-flag">∞</span>{/if}
          </span>
        </div>

        {#if snapshot.schedule.fidelity_warnings.length > 0}
          <div class="fidelity">
            <span class="fidelity-icon"><WarningAlt size={16} /></span>
            <ul>
              {#each snapshot.schedule.fidelity_warnings as note}
                <li>{note}</li>
              {/each}
            </ul>
          </div>
        {/if}
      </main>

      <aside class="pane inspector-pane">
        <div class="pane-head">
          <h2>Properties</h2>
          <span class="spacer"></span>
          <span class="pane-context mono">desc.txt</span>
        </div>

        <div class="inspector-scroll">
          <Accordion>
            <AccordionItem title="Animation" open>
              <div class="field-grid">
                <NumberInput
                  size="sm"
                  labelText="Width (px)"
                  min={1}
                  max={16384}
                  value={snapshot.manifest.descriptor.width}
                  on:change={(event) => updateHeaderNumber('width', Number(event.detail ?? 0))}
                />
                <NumberInput
                  size="sm"
                  labelText="Height (px)"
                  min={1}
                  max={16384}
                  value={snapshot.manifest.descriptor.height}
                  on:change={(event) => updateHeaderNumber('height', Number(event.detail ?? 0))}
                />
                <NumberInput
                  size="sm"
                  labelText="Frame rate (fps)"
                  min={1}
                  max={240}
                  value={snapshot.manifest.descriptor.fps}
                  on:change={(event) => updateHeaderNumber('fps', Number(event.detail ?? 0))}
                />
                <div class="toggle-field">
                  <Toggle
                    size="sm"
                    labelText="Show boot progress"
                    toggled={snapshot.manifest.descriptor.show_progress}
                    on:toggle={(event) => toggleProgressFlag(event.detail.toggled)}
                  />
                  <p class="hint">Draws Android's boot percentage over the last part.</p>
                </div>
              </div>
            </AccordionItem>

            {#if activePartDecl}
              <AccordionItem title={`Part · ${activePartDecl.path}`} open>
                <div class="field-grid">
                  <Select
                    size="sm"
                    labelText="Playback mode"
                    selected={activePartDecl.mode.mode === 'fade_out'
                      ? 'f'
                      : activePartDecl.mode.mode === 'play_to_completion'
                        ? 'c'
                        : activePartDecl.mode.mode === 'system_fallback'
                          ? 'system'
                          : 'p'}
                    on:update={(event) => updatePartMode(String(event.detail))}
                  >
                    <SelectItem value="p" text="p · interruptible on boot exit" />
                    <SelectItem value="c" text="c · play to completion" />
                    <SelectItem value="f" text="f · fade out when interrupted" />
                    <SelectItem value="system" text="$SYSTEM · device fallback" />
                  </Select>

                  <NumberInput
                    size="sm"
                    labelText="Repeat count"
                    helperText="0 loops until boot completes"
                    min={0}
                    max={999}
                    value={activePartDecl.repeat.kind === 'infinite' ? 0 : activePartDecl.repeat.count}
                    on:change={(event) => updatePartRepeat(Number(event.detail ?? 0))}
                  />
                  <NumberInput
                    size="sm"
                    labelText="Pause hold"
                    helperText="Measured in frames"
                    min={0}
                    max={10000}
                    value={activePartDecl.pause_frames}
                    on:change={(event) => updatePartPause(Number(event.detail ?? 0))}
                  />

                  <div class="color-field">
                    <TextInput
                      size="sm"
                      labelText="Background colour"
                      placeholder="#000000"
                      class="mono"
                      value={formatRgbHex(activePartDecl.background_color)}
                      on:change={(event) => updatePartBgHex(String(event.detail ?? ''))}
                    />
                    <span
                      class="swatch"
                      style="background: {formatRgbHex(activePartDecl.background_color)}"
                      aria-hidden="true"
                    ></span>
                  </div>
                </div>
              </AccordionItem>
            {/if}

            <AccordionItem title="Build transformations">
              <p class="hint">
                Source frames are never rewritten. These steps run only while the archive is built.
              </p>
              <div class="toggle-stack">
                <Toggle
                  size="sm"
                  labelText="Lossless PNG optimisation"
                  toggled={snapshot.manifest.transforms.lossless_optimize_png}
                  on:toggle={(event) => toggleTransform('lossless_optimize_png', event.detail.toggled)}
                />
                <Toggle
                  size="sm"
                  labelText="Generate trim.txt per part"
                  toggled={snapshot.manifest.transforms.generate_trim_txt}
                  on:toggle={(event) => toggleTransform('generate_trim_txt', event.detail.toggled)}
                />
              </div>
            </AccordionItem>

            <AccordionItem title="Magisk module">
              <div class="field-grid">
                <TextInput
                  size="sm"
                  labelText="Module ID"
                  class="mono"
                  value={snapshot.manifest.magisk.module_id}
                  on:change={(event) => commitMagiskId(String(event.detail ?? ''))}
                />
                <Select
                  size="sm"
                  labelText="Overlay target"
                  selected={snapshot.manifest.magisk.target_path}
                  on:update={(event) => commitMagiskTarget(String(event.detail))}
                >
                  <SelectItem
                    value="system/media/bootanimation.zip"
                    text="/system/media · AOSP, LineageOS"
                  />
                  <SelectItem
                    value="system/product/media/bootanimation.zip"
                    text="/system/product/media · Pixel, Treble"
                  />
                  <SelectItem
                    value="system/system_ext/media/bootanimation.zip"
                    text="/system/system_ext/media · OEM"
                  />
                </Select>
              </div>
            </AccordionItem>
          </Accordion>
        </div>
      </aside>
    </div>

    <section class="dock">
      <Tabs bind:selected={dockTab}>
        <Tab label={`Timeline · ${partFrameCount}`} />
        <Tab label={`Diagnostics · ${diagnostics.length}`} />
        <svelte:fragment slot="content">
          <TabContent>
                <div class="timeline">
              <div class="timeline-bar">
              <span class="timeline-label mono">{activeProjectPart?.name ?? 'no part'}</span>
              <span class="spacer"></span>
              <Button
                kind="ghost"
                size="small"
                icon={ChevronLeft}
                iconDescription="Move frame left"
                disabled={selectedFrameIdx === 0}
                onclick={() => moveSelectedFrame(-1)}
              ></Button>
              <Button
                kind="ghost"
                size="small"
                icon={ChevronRight}
                iconDescription="Move frame right"
                disabled={selectedFrameIdx >= Math.max(0, partFrameCount - 1)}
                onclick={() => moveSelectedFrame(1)}
              ></Button>
              <Button
                kind="ghost"
                size="small"
                disabled={partFrameCount === 0}
                onclick={duplicateSelectedFrame}
                icon={Copy}
              >
                Duplicate
              </Button>
              <Button kind="ghost" size="small" disabled={partFrameCount <= 1} onclick={removeSelectedFrame} icon={TrashCan}>Remove</Button>
            </div>

            <div class="filmstrip">
              {#if activeProjectPart}
                {#each activeProjectPart.frames as frame, idx (frame.relative_path + idx)}
                  <button
                    type="button"
                    class="clip"
                    class:is-active={idx === selectedFrameIdx}
                    onclick={() => selectFrame(idx)}
                  >
                    <span class="clip-head mono">{String(idx + 1).padStart(3, '0')}</span>
                    <span class="clip-thumb"><ImageIcon size={18} /></span>
                    <span class="clip-name mono">{frame.relative_path.split('/').pop()}</span>
                  </button>
                {/each}
              {:else}
                <p class="empty">Select a part to see its frames.</p>
              {/if}
            </div>
          </div>
          </TabContent>

          <TabContent>
            <div class="diagnostics">
            {#if totalFrames === 0}
              <div class="clean-state neutral">
                <Information size={20} />
                <div>
                  <p class="clean-title">Nothing to validate yet</p>
                  <p class="hint">
                    Open a project folder to check its archive structure and desc.txt semantics.
                  </p>
                </div>
              </div>
            {:else if diagnostics.length === 0}
              <div class="clean-state">
                <CheckmarkFilled size={20} />
                <div>
                  <p class="clean-title">No findings</p>
                  <p class="hint">
                    Archive structure and desc.txt semantics satisfy every invariant.
                  </p>
                </div>
              </div>
            {:else}
              {#each diagnostics as diag (diag.code + diag.message)}
                <button
                  type="button"
                  class="finding"
                  class:is-error={diag.severity === 'error' || diag.blocks_export}
                  onclick={() => focusDiagnosticTarget(diag)}
                >
                  <span class="finding-icon">
                    {#if diag.severity === 'error' || diag.blocks_export}
                      <ErrorFilled size={16} />
                    {:else if diag.severity === 'warning'}
                      <WarningAlt size={16} />
                    {:else}
                      <Information size={16} />
                    {/if}
                  </span>
                  <span class="finding-body">
                    <span class="finding-line">
                      <span class="finding-code mono">{diag.code}</span>
                      <span class="finding-message">{diag.message}</span>
                    </span>
                    <span class="finding-fix">{diag.remediation}</span>
                  </span>
                  {#if diag.blocks_export}
                    <span class="finding-flag">blocks export</span>
                  {/if}
                </button>
              {/each}
            {/if}
            </div>
          </TabContent>
        </svelte:fragment>
      </Tabs>
    </section>

    <footer class="status-bar">
      <span class="status-message">{statusBanner}</span>
      <span class="status-meta mono">
        {totalFrames} frames
        <span class="status-sep">|</span>
        {(snapshot.validation.performance.total_image_bytes / 1024).toFixed(1)} KiB
        <span class="status-sep">|</span>
        ZIP_STORED (0)
      </span>
    </footer>

    <Modal
      open={showExportModal}
      modalHeading="Build and export"
      modalLabel="Artifact"
      primaryButtonText="Verify and write"
      secondaryButtonText="Cancel"
      primaryButtonDisabled={snapshot.validation.outcome === 'invalid' || totalFrames === 0}
      primaryButtonLoading={exporting}
      on:click:button--primary={executeReviewedExport}
      on:click:button--secondary={() => (showExportModal = false)}
      on:close={() => (showExportModal = false)}
    >
      <div class="export-form">
        <Select
          size="sm"
          labelText="Export type"
          selected={exportTargetKind}
          on:update={(event) => setExportKind(String(event.detail))}
        >
          <SelectItem value="bootanimation" text="Standalone bootanimation.zip" />
          <SelectItem value="magisk" text="Flashable Magisk module (.zip)" />
        </Select>

        <div class="destination">
          <TextInput
            size="sm"
            labelText="Output destination"
            class="mono"
            bind:value={exportDestination}
          />
          <Button
            kind="tertiary"
            size="small"
            disabled={!bridge.isDesktop}
            onclick={handleBrowseDestination}
          >
            Browse…
          </Button>
        </div>

        <dl class="summary mono">
          <div>
            <dt>Geometry</dt>
            <dd>
              {snapshot.manifest.descriptor.width}×{snapshot.manifest.descriptor.height}
              @ {snapshot.manifest.descriptor.fps} fps
            </dd>
          </div>
          <div>
            <dt>Content</dt>
            <dd>{snapshot.manifest.parts.length} parts · {totalFrames} frames</dd>
          </div>
          <div>
            <dt>Storage</dt>
            <dd>ZIP_STORED, method 0, verified on reopen</dd>
          </div>
          <div>
            <dt>Transformations</dt>
            <dd>
              {snapshot.manifest.transforms.lossless_optimize_png ? 'Lossless PNG' : '—'}
              {snapshot.manifest.transforms.generate_trim_txt ? ' + trim.txt' : ''}
            </dd>
          </div>
          <div>
            <dt>Validation</dt>
            <dd class="validation-value {snapshot.validation.outcome}">
              {snapshot.validation.outcome.replaceAll('_', ' ')}
            </dd>
          </div>
          {#if exportTargetKind === 'magisk'}
            <div>
              <dt>Module</dt>
              <dd>{snapshot.manifest.magisk.module_id}</dd>
            </div>
            <div>
              <dt>Overlay</dt>
              <dd>/{snapshot.manifest.magisk.target_path}</dd>
            </div>
          {/if}
        </dl>

        {#if snapshot.validation.outcome === 'invalid'}
          <InlineNotification
            kind="error"
            lowContrast
            hideCloseButton
            title="Blocking diagnostics"
            subtitle="Export stays disabled until every blocking finding is resolved."
          />
        {/if}

        {#if buildResult}
          <InlineNotification
            kind={buildResult.kind}
            lowContrast
            hideCloseButton
            title={buildResult.title}
            subtitle={buildResult.text}
          />
        {/if}
      </div>
    </Modal>
  </div>
{:else}
  <div class="boot">
    <Loading description="Opening studio session" withOverlay={false} small />
  </div>
{/if}

<style>
  .studio {
    display: grid;
    grid-template-rows: 48px minmax(0, 1fr) 216px 24px;
    width: 100%;
    height: 100%;
    background: var(--studio-surface);
  }

  .boot {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
  }

  /* ---------------------------------------------------------------- header */

  .app-header {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 0 12px 0 14px;
    background: #161616;
    border-bottom: 1px solid var(--studio-border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .brand-mark {
    display: block;
    width: 26px;
    height: 26px;
    background: #0b0b0b;
    border-radius: 50%;
    box-shadow: 0 0 0 1px #3d3d3d;
  }

  .brand-name {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.1px;
  }

  .brand-product {
    font-size: 14px;
    font-weight: 400;
    color: var(--cds-text-02, #c6c6c6);
  }

  .version {
    margin-left: 2px;
    padding: 1px 5px;
    font-size: 10px;
    line-height: 14px;
    color: var(--cds-text-05, #8d8d8d);
    border: 1px solid var(--studio-border);
  }

  .project-id {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding-left: 20px;
    border-left: 1px solid var(--studio-border);
  }

  .project-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--cds-text-02, #c6c6c6);
    font-size: 12px;
  }

  .unsaved {
    padding: 0 6px;
    font-size: 10px;
    line-height: 16px;
    color: #f1c21b;
    background: rgba(241, 194, 27, 0.12);
    border: 1px solid rgba(241, 194, 27, 0.32);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
  }

  .divider {
    width: 1px;
    height: 20px;
    margin: 0 8px;
    background: var(--studio-border);
  }

  .validation-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    margin-right: 10px;
    padding: 0 8px;
    font-size: 11px;
    font-weight: 500;
    border: 1px solid transparent;
  }

  .validation-chip :global(svg) {
    fill: currentColor;
  }

  .validation-chip.valid {
    color: #42be65;
    background: rgba(66, 190, 101, 0.1);
    border-color: rgba(66, 190, 101, 0.28);
  }

  .validation-chip.valid_with_warnings {
    color: #f1c21b;
    background: rgba(241, 194, 27, 0.1);
    border-color: rgba(241, 194, 27, 0.28);
  }

  .validation-chip.invalid {
    color: #fa4d56;
    background: rgba(250, 77, 86, 0.1);
    border-color: rgba(250, 77, 86, 0.3);
  }

  .validation-chip.empty {
    color: var(--cds-text-04, #a8a8a8);
    background: #262626;
    border-color: #393939;
  }

  /* ------------------------------------------------------------- workspace */

  .workspace {
    display: grid;
    grid-template-columns: 264px minmax(0, 1fr) 336px;
    min-height: 0;
    overflow: hidden;
  }

  .pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    background: #161616;
  }

  .parts-pane {
    border-right: 1px solid var(--studio-border);
  }

  .inspector-pane {
    border-left: 1px solid var(--studio-border);
  }

  .pane-head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 10px 0 12px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--studio-divider);
  }

  .pane-head.sub {
    border-top: 1px solid var(--studio-divider);
  }

  .pane-head h2 {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.16px;
    color: var(--cds-text-01, #f4f4f4);
  }

  .count {
    min-width: 18px;
    padding: 0 4px;
    font-size: 10px;
    line-height: 16px;
    text-align: center;
    color: var(--cds-text-05, #8d8d8d);
    background: #262626;
  }

  .pane-context {
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .spacer {
    flex: 1;
  }

  .part-list {
    display: flex;
    flex-direction: column;
    padding: 4px;
    gap: 1px;
  }

  .part-row {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    text-align: left;
    color: var(--cds-text-01, #f4f4f4);
    background: transparent;
    border: 0;
    border-left: 2px solid transparent;
    cursor: pointer;
  }

  .part-row:hover {
    background: #262626;
  }

  .part-row.is-selected {
    background: #262626;
    border-left-color: #4589ff;
  }

  .mode-tag {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 18px;
    font-size: 10px;
    color: var(--cds-text-02, #c6c6c6);
    border: 1px solid #525252;
  }

  .part-label {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .part-name {
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .part-sub {
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .part-count {
    font-size: 10px;
    color: var(--cds-text-03, #6f6f6f);
  }

  .new-part {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    padding: 8px 10px 10px 12px;
    border-bottom: 1px solid var(--studio-divider);
  }

  .new-part :global(.bx--form-item) {
    flex: 1;
    min-width: 0;
  }

  .frame-search {
    padding: 8px 10px 8px 12px;
    flex-shrink: 0;
  }

  .frame-search :global(.bx--search) {
    border-bottom: 1px solid #393939;
  }

  .frame-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 4px 8px;
  }

  .frame-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 8px;
    text-align: left;
    background: transparent;
    border: 0;
    border-left: 2px solid transparent;
    cursor: pointer;
  }

  .frame-row:hover {
    background: #262626;
  }

  .frame-row.is-selected {
    background: #262626;
    border-left-color: #4589ff;
  }

  .frame-index {
    font-size: 10px;
    color: var(--cds-text-03, #6f6f6f);
  }

  .frame-path {
    flex: 1;
    overflow: hidden;
    font-size: 11px;
    color: var(--cds-text-02, #c6c6c6);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .frame-trim {
    font-size: 9px;
    color: var(--cds-text-03, #6f6f6f);
  }

  .empty {
    padding: 12px;
    font-size: 11px;
    color: var(--cds-text-05, #8d8d8d);
  }

  /* ------------------------------------------------------------ stage pane */

  .stage-pane {
    background: #1c1c1c;
  }

  .stage-head {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 40px;
    padding: 0 12px 0 16px;
    flex-shrink: 0;
    overflow: hidden;
    border-bottom: 1px solid var(--studio-divider);
    background: #161616;
  }

  .stage-head h2 {
    font-size: 12px;
    font-weight: 600;
  }

  .stage-controls {
    display: flex;
    align-items: center;
    gap: 14px;
    padding-left: 14px;
    border-left: 1px solid var(--studio-border);
  }

  .stage-controls :global(.bx--select) {
    width: auto;
  }

  .stage-controls :global(.bx--select-input) {
    padding-right: 28px;
    background: #1c1c1c;
  }

  .stage-meta {
    margin-left: auto;
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .stage {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 0;
    padding: 16px;
    overflow: hidden;
    background-color: #141414;
    background-image:
      linear-gradient(rgba(255, 255, 255, 0.022) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255, 255, 255, 0.022) 1px, transparent 1px);
    background-size: 24px 24px;
    background-position: center;
  }

  .device {
    padding: 12px;
    background: #0b0b0b;
    border: 1px solid #393939;
    border-radius: 14px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.55);
  }

  .device canvas {
    display: block;
    border-radius: 6px;
    background: #080808;
  }

  .stage-note {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    color: var(--cds-text-03, #6f6f6f);
  }

  .stage-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    max-width: 320px;
    padding: 20px 24px;
    text-align: center;
    border: 1px dashed #393939;
    background: rgba(22, 22, 22, 0.72);
  }

  .stage-empty-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    margin-bottom: 2px;
    color: var(--cds-text-04, #a8a8a8);
    border: 1px solid #393939;
  }

  .stage-empty-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--cds-text-01, #f4f4f4);
  }

  .stage-empty-body {
    font-size: 11px;
    line-height: 1.6;
    color: var(--cds-text-05, #8d8d8d);
  }

  .stage-empty-hint {
    margin-top: 2px;
  }

  @media (max-width: 1180px) {
    .stage-head {
      flex-wrap: wrap;
      height: auto;
      min-height: 40px;
      padding: 6px 12px;
      row-gap: 6px;
    }

    .stage-controls {
      flex-wrap: wrap;
      gap: 6px 12px;
      padding-left: 10px;
    }

    .stage-meta {
      display: none;
    }
  }

  @media (max-height: 720px) {
    .studio {
      grid-template-rows: 48px minmax(0, 1fr) 176px 24px;
    }

    .stage {
      gap: 6px;
      padding: 8px;
    }

    .stage-note {
      display: none;
    }
  }

  .stage-note :global(svg) {
    fill: currentColor;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    flex-shrink: 0;
    border-top: 1px solid var(--studio-divider);
    background: #161616;
  }

  .scrub {
    flex: 1;
    min-width: 120px;
    padding: 0 4px;
  }

  .scrub :global(.bx--slider) {
    width: 100%;
    min-width: 0;
    max-width: none;
    margin: 0;
  }

  .scrub :global(.bx--slider-container) {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-width: 0;
  }

  .scrub :global(.bx--slider__track) {
    min-width: 0;
  }

  .scrub :global(.bx--slider__range-label) {
    display: none;
  }

  .readout {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: flex-end;
    gap: 5px;
    font-size: 10px;
    color: var(--cds-text-02, #c6c6c6);
    white-space: nowrap;
  }

  .readout-divider {
    color: var(--cds-text-03, #6f6f6f);
  }

  .readout-flag {
    color: #78a9ff;
  }

  .fidelity {
    display: flex;
    gap: 8px;
    max-height: 66px;
    overflow-y: auto;
    padding: 6px 14px 8px;
    font-size: 10px;
    color: #f1c21b;
    background: rgba(241, 194, 27, 0.06);
    border-top: 1px solid rgba(241, 194, 27, 0.2);
  }

  .fidelity-icon {
    padding-top: 1px;
  }

  .fidelity-icon :global(svg) {
    fill: currentColor;
  }

  .fidelity ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* -------------------------------------------------------------- inspector */

  .inspector-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 12px 16px;
  }

  .inspector-scroll :global(.bx--accordion__item) {
    border-bottom: 1px solid var(--studio-divider);
  }

  .inspector-scroll :global(.bx--accordion__heading) {
    height: 36px;
  }

  .inspector-scroll :global(.bx--accordion__title) {
    font-size: 12px;
    color: var(--cds-text-01, #f4f4f4);
  }

  .inspector-scroll :global(.bx--accordion__content) {
    padding-right: 4px;
    padding-left: 4px;
  }

  .field-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 14px;
  }

  .field-grid :global(.bx--label) {
    font-size: 11px;
    color: var(--cds-text-02, #c6c6c6);
  }

  .field-grid :global(.bx--form__helper-text) {
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .field-grid :global(.bx--number) {
    width: 100%;
  }

  .toggle-field {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle-stack {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 14px;
  }

  .color-field {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }

  .color-field :global(.bx--form-item) {
    flex: 1;
  }

  .swatch {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border: 1px solid #525252;
  }

  .hint {
    font-size: 10px;
    line-height: 1.5;
    color: var(--cds-text-05, #8d8d8d);
  }

  /* ------------------------------------------------------------------- dock */

  .dock {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--studio-border);
    background: #161616;
  }

  .dock :global(.bx--tabs) {
    flex: 0 0 36px;
    height: 36px;
  }

  .dock :global(.bx--tabs__nav) {
    padding-left: 8px;
  }

  .dock :global(.bx--tabs__nav-item) {
    width: auto;
  }

  .dock :global(.bx--tabs__nav-link) {
    height: 36px;
    padding: 0 14px;
    font-size: 12px;
    line-height: 36px;
    border-bottom: 2px solid transparent;
  }

  .dock :global(.bx--tabs__nav-item--selected .bx--tabs__nav-link) {
    border-bottom-color: #f4f4f4;
  }

  .dock :global(.bx--tab-content) {
    flex: 1;
    min-height: 0;
    padding: 0;
    overflow: hidden;
  }

  .timeline {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .timeline-bar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 8px 4px 12px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--studio-divider);
  }

  .timeline-label {
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .filmstrip {
    display: flex;
    align-items: stretch;
    gap: 6px;
    flex: 1;
    min-height: 0;
    padding: 8px 10px 10px;
    overflow-x: auto;
  }

  .clip {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 104px;
    min-height: 88px;
    flex-shrink: 0;
    padding: 6px;
    text-align: left;
    background: #1c1c1c;
    border: 1px solid #2a2a2a;
    cursor: pointer;
  }

  .clip:hover {
    border-color: #525252;
  }

  .clip.is-active {
    border-color: #4589ff;
    box-shadow: inset 0 0 0 1px #4589ff;
  }

  .clip-head {
    font-size: 9px;
    color: var(--cds-text-03, #6f6f6f);
  }

  .clip-thumb {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    min-height: 40px;
    color: #525252;
    background-color: #0f0f0f;
    background-image:
      linear-gradient(45deg, #1a1a1a 25%, transparent 25%),
      linear-gradient(-45deg, #1a1a1a 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, #1a1a1a 75%),
      linear-gradient(-45deg, transparent 75%, #1a1a1a 75%);
    background-size: 12px 12px;
    background-position: 0 0, 0 6px, 6px -6px, -6px 0;
  }

  .clip.is-active .clip-thumb {
    color: #78a9ff;
  }

  .clip-name {
    overflow: hidden;
    font-size: 9px;
    color: var(--cds-text-02, #c6c6c6);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diagnostics {
    height: 100%;
    overflow-y: auto;
    padding: 4px 6px 8px;
  }

  .clean-state {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 12px;
    color: #42be65;
  }

  .clean-state.neutral {
    color: var(--cds-text-04, #a8a8a8);
  }

  .clean-title {
    font-size: 12px;
    color: #42be65;
  }

  .clean-state.neutral .clean-title {
    color: var(--cds-text-02, #c6c6c6);
  }

  .finding {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    text-align: left;
    background: transparent;
    border: 0;
    border-left: 2px solid rgba(241, 194, 27, 0.7);
    cursor: pointer;
  }

  .finding:hover {
    background: #262626;
  }

  .finding.is-error {
    border-left-color: #fa4d56;
  }

  .finding-icon {
    padding-top: 1px;
    color: #f1c21b;
  }

  .finding.is-error .finding-icon {
    color: #fa4d56;
  }

  .finding-icon :global(svg) {
    fill: currentColor;
  }

  .finding-body {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .finding-line {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .finding-code {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--cds-text-02, #c6c6c6);
  }

  .finding-message {
    font-size: 11px;
    color: var(--cds-text-01, #f4f4f4);
  }

  .finding-fix {
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .finding-flag {
    margin-left: auto;
    flex-shrink: 0;
    padding: 0 5px;
    font-size: 9px;
    line-height: 16px;
    color: #fa4d56;
    border: 1px solid rgba(250, 77, 86, 0.35);
  }

  /* -------------------------------------------------------------- statusbar */

  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 0 12px;
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
    background: #0f0f0f;
    border-top: 1px solid var(--studio-divider);
  }

  .status-message {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-meta {
    display: inline-flex;
    gap: 8px;
    white-space: nowrap;
  }

  .status-sep {
    color: #393939;
  }

  /* ----------------------------------------------------------------- modal */

  .export-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: 4px;
  }

  .destination {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }

  .destination :global(.bx--form-item) {
    flex: 1;
    min-width: 0;
  }

  .summary {
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid var(--studio-border);
    background: #161616;
  }

  .summary div {
    display: grid;
    grid-template-columns: 132px minmax(0, 1fr);
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--studio-divider);
  }

  .summary div:last-child {
    border-bottom: 0;
  }

  .summary dt {
    font-size: 10px;
    color: var(--cds-text-05, #8d8d8d);
  }

  .summary dd {
    font-size: 10px;
    color: var(--cds-text-02, #c6c6c6);
  }

  .validation-value.valid {
    color: #42be65;
  }

  .validation-value.valid_with_warnings {
    color: #f1c21b;
  }

  .validation-value.invalid {
    color: #fa4d56;
  }
</style>
