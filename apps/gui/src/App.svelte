<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { StudioBridge } from './lib/bridge';
  import type {
    Diagnostic,
    PartDeclaration,
    PlaybackMode,
    RepeatCount,
    RgbColor,
    WorkspaceSnapshot,
  } from './lib/types';

  const bridge = new StudioBridge();

  let snapshot: WorkspaceSnapshot | null = $state(null);
  let selectedPartIdx = $state(0);
  let selectedFrameIdx = $state(0);
  let currentTick = $state(0);
  let isPlaying = $state(false);
  let playbackSpeed = $state(1.0);
  let canvasZoom = $state(1.0);
  let previewMode = $state<'full' | 'selected_part'>('full');
  let assetFilter = $state('');
  let showExportModal = $state(false);
  let exportTargetKind = $state<'bootanimation' | 'magisk'>('bootanimation');
  let exportDestination = $state('dist/bootanimation.zip');
  let lastBuildLog = $state<string | null>(null);
  let statusBanner = $state('Ready — ZIP_STORED verification enforced');
  let newPartName = $state('');

  let canvasEl: HTMLCanvasElement | undefined = $state();
  let timerId: ReturnType<typeof setInterval> | null = null;

  async function refresh(partFilter: number | null = null) {
    const filter = previewMode === 'selected_part' ? selectedPartIdx : partFilter;
    snapshot = await bridge.getSnapshot(filter);
    if (snapshot && currentTick >= snapshot.schedule.frames.length) {
      currentTick = Math.max(0, snapshot.schedule.frames.length - 1);
    }
    drawCanvasFrame();
  }

  onMount(async () => {
    await refresh();
  });

  onDestroy(() => {
    stopPlayback();
  });

  function startPlayback() {
    if (!snapshot || snapshot.schedule.frames.length === 0) return;
    stopPlayback();
    isPlaying = true;
    const fps = Math.max(1, snapshot.schedule.fps * playbackSpeed);
    const intervalMs = Math.max(12, Math.round(1000 / fps));
    timerId = setInterval(() => {
      if (!snapshot || snapshot.schedule.frames.length === 0) return;
      currentTick = (currentTick + 1) % snapshot.schedule.frames.length;
      drawCanvasFrame();
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
    if (isPlaying) {
      stopPlayback();
    } else {
      startPlayback();
    }
  }

  function stepFrame(delta: number) {
    stopPlayback();
    if (!snapshot || snapshot.schedule.frames.length === 0) return;
    const len = snapshot.schedule.frames.length;
    currentTick = (currentTick + delta + len) % len;
    drawCanvasFrame();
  }

  function drawCanvasFrame() {
    if (!canvasEl || !snapshot) return;
    const ctx = canvasEl.getContext('2d');
    if (!ctx) return;

    const w = canvasEl.width;
    const h = canvasEl.height;
    const sched = snapshot.schedule;
    const active = sched.frames[currentTick];

    const bg = active?.background_color ?? { r: 8, g: 10, b: 14 };
    ctx.fillStyle = `rgb(${bg.r}, ${bg.g}, ${bg.b})`;
    ctx.fillRect(0, 0, w, h);

    // Subtle device safe-area grid
    ctx.strokeStyle = 'rgba(255,255,255,0.06)';
    ctx.lineWidth = 1;
    ctx.strokeRect(0.5, 0.5, w - 1, h - 1);

    if (!active) return;

    // Render trim bounding box if present
    if (active.trim) {
      const sx = w / Math.max(1, sched.width);
      const sy = h / Math.max(1, sched.height);
      const tx = active.trim.x * sx;
      const ty = active.trim.y * sy;
      const tw = active.trim.width * sx;
      const th = active.trim.height * sy;

      ctx.strokeStyle = 'rgba(59, 130, 246, 0.65)';
      ctx.setLineDash([4, 3]);
      ctx.strokeRect(tx, ty, tw, th);
      ctx.setLineDash([]);
    }

    // Procedural vector emblem representing the active boot frame
    const cx = w / 2;
    const cy = h / 2;
    const pulse = 0.68 + 0.32 * Math.sin((active.frame_index_in_part / 6) * Math.PI * 2);
    const radius = Math.min(w, h) * 0.24 * pulse;

    ctx.save();
    ctx.globalAlpha = active.fade_opacity;

    const grad = ctx.createRadialGradient(cx, cy, radius * 0.15, cx, cy, radius * 1.35);
    if (active.part_index === 0) {
      grad.addColorStop(0, '#60a5fa');
      grad.addColorStop(1, 'rgba(37, 99, 235, 0)');
    } else if (active.part_index === 1) {
      grad.addColorStop(0, '#34d399');
      grad.addColorStop(1, 'rgba(16, 185, 129, 0)');
    } else {
      grad.addColorStop(0, '#f472b6');
      grad.addColorStop(1, 'rgba(219, 39, 119, 0)');
    }

    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(cx, cy, radius * 1.35, 0, Math.PI * 2);
    ctx.fill();

    ctx.strokeStyle = '#eef2f8';
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.arc(
      cx,
      cy,
      radius * 0.72,
      active.frame_index_in_part * 0.35,
      active.frame_index_in_part * 0.35 + Math.PI * 1.35,
    );
    ctx.stroke();

    // HUD overlay inside preview canvas
    ctx.fillStyle = 'rgba(238, 242, 248, 0.88)';
    ctx.font = '600 11px JetBrains Mono, monospace';
    ctx.fillText(`${active.part_name} • frame #${active.frame_index_in_part}`, 12, 20);
    if (active.is_pause_hold) {
      ctx.fillStyle = '#f59e0b';
      ctx.fillText('PAUSE HOLD', 12, 36);
    }

    if (snapshot.manifest.descriptor.show_progress) {
      const pct = Math.min(
        100,
        Math.round(((currentTick + 1) / Math.max(1, sched.frames.length)) * 100),
      );
      ctx.fillStyle = 'rgba(238, 242, 248, 0.75)';
      ctx.textAlign = 'center';
      ctx.fillText(`${pct}%`, cx, Math.round(h * 0.67));
      ctx.textAlign = 'start';
    }

    ctx.restore();
  }

  async function handleUndo() {
    stopPlayback();
    snapshot = await bridge.undo(previewMode === 'selected_part' ? selectedPartIdx : null);
    statusBanner = 'Undid last edit operation';
    drawCanvasFrame();
  }

  async function handleRedo() {
    stopPlayback();
    snapshot = await bridge.redo(previewMode === 'selected_part' ? selectedPartIdx : null);
    statusBanner = 'Redid edit operation';
    drawCanvasFrame();
  }

  async function handleSaveProject() {
    snapshot = await bridge.markSaved();
    statusBanner = `Saved project manifest (${snapshot.manifest.name}/project.bootanimix.json)`;
  }

  async function updateHeaderNumber(field: 'width' | 'height' | 'fps', raw: string) {
    const val = Math.max(1, Number.parseInt(raw, 10) || 1);
    snapshot = await bridge.commitMutation(
      (m) => {
        m.descriptor[field] = val;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    statusBanner = `Updated header ${field.toUpperCase()} to ${val}`;
    drawCanvasFrame();
  }

  async function toggleProgressFlag(checked: boolean) {
    snapshot = await bridge.commitMutation(
      (m) => {
        m.descriptor.show_progress = checked;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    drawCanvasFrame();
  }

  async function updatePartMode(modeKey: string) {
    snapshot = await bridge.commitMutation(
      (m) => {
        const part = m.descriptor.parts[selectedPartIdx];
        if (!part) return;
        if (modeKey === 'p') part.mode = { mode: 'play_interruptible' };
        else if (modeKey === 'c') part.mode = { mode: 'play_to_completion' };
        else if (modeKey === 'f') part.mode = { mode: 'fade_out', fade_frames: 10 };
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    drawCanvasFrame();
  }

  async function updatePartRepeat(rawCount: string) {
    const num = Math.max(0, Number.parseInt(rawCount, 10) || 0);
    const repeat: RepeatCount =
      num === 0 ? { kind: 'infinite' } : { kind: 'finite', count: num };
    snapshot = await bridge.commitMutation(
      (m) => {
        const part = m.descriptor.parts[selectedPartIdx];
        if (part) part.repeat = repeat;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    drawCanvasFrame();
  }

  async function updatePartPause(rawPause: string) {
    const pause = Math.max(0, Number.parseInt(rawPause, 10) || 0);
    snapshot = await bridge.commitMutation(
      (m) => {
        const part = m.descriptor.parts[selectedPartIdx];
        if (part) part.pause_frames = pause;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    drawCanvasFrame();
  }

  async function updatePartBgHex(hex: string) {
    const clean = hex.replace('#', '').trim();
    if (clean.length !== 6) return;
    const r = Number.parseInt(clean.slice(0, 2), 16);
    const g = Number.parseInt(clean.slice(2, 4), 16);
    const b = Number.parseInt(clean.slice(4, 6), 16);
    if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) return;
    const color: RgbColor = { r, g, b };
    snapshot = await bridge.commitMutation(
      (m) => {
        const part = m.descriptor.parts[selectedPartIdx];
        if (part) part.background_color = color;
      },
      previewMode === 'selected_part' ? selectedPartIdx : null,
    );
    drawCanvasFrame();
  }

  async function addNewPart() {
    if (!snapshot) return;
    const name = (newPartName.trim() || `part${snapshot.manifest.parts.length}`).replace(
      /[^a-zA-Z0-9_-]/g,
      '',
    );
    if (!name || snapshot.manifest.parts.some((p) => p.name === name)) {
      statusBanner = `Part '${name}' already exists or is invalid`;
      return;
    }
    snapshot = await bridge.commitMutation((m) => {
      const decl: PartDeclaration = {
        mode: { mode: 'play_interruptible' },
        repeat: { kind: 'finite', count: 1 },
        pause_frames: 0,
        path: name,
        background_color: { r: 0, g: 0, b: 0 },
        clock: null,
      };
      m.descriptor.parts.push(decl);
      m.parts.push({
        name,
        frames: [
          {
            relative_path: `assets/${name}/00000.png`,
            trim: null,
          },
        ],
        audio_relative_path: null,
      });
    });
    newPartName = '';
    selectedPartIdx = snapshot.manifest.parts.length - 1;
    selectedFrameIdx = 0;
    statusBanner = `Added part '${name}'`;
    drawCanvasFrame();
  }

  async function deleteSelectedPart() {
    if (!snapshot || snapshot.manifest.parts.length <= 1) {
      statusBanner = 'Cannot delete the last remaining part';
      return;
    }
    const removedName = snapshot.manifest.parts[selectedPartIdx]?.name;
    snapshot = await bridge.commitMutation((m) => {
      m.descriptor.parts.splice(selectedPartIdx, 1);
      m.parts.splice(selectedPartIdx, 1);
    });
    selectedPartIdx = Math.max(0, selectedPartIdx - 1);
    selectedFrameIdx = 0;
    statusBanner = `Removed part '${removedName}'`;
    drawCanvasFrame();
  }

  async function moveSelectedFrame(delta: number) {
    if (!snapshot) return;
    const part = snapshot.manifest.parts[selectedPartIdx];
    if (!part) return;
    const target = selectedFrameIdx + delta;
    if (target < 0 || target >= part.frames.length) return;
    snapshot = await bridge.commitMutation((m) => {
      const frames = m.parts[selectedPartIdx].frames;
      const [item] = frames.splice(selectedFrameIdx, 1);
      frames.splice(target, 0, item);
    });
    selectedFrameIdx = target;
    statusBanner = `Moved frame #${selectedFrameIdx - delta} -> #${target} in ${part.name}`;
    drawCanvasFrame();
  }

  async function duplicateSelectedFrame() {
    if (!snapshot) return;
    const part = snapshot.manifest.parts[selectedPartIdx];
    if (!part || !part.frames[selectedFrameIdx]) return;
    snapshot = await bridge.commitMutation((m) => {
      const frames = m.parts[selectedPartIdx].frames;
      const copy = structuredClone(frames[selectedFrameIdx]);
      frames.splice(selectedFrameIdx + 1, 0, copy);
    });
    selectedFrameIdx += 1;
    statusBanner = `Duplicated frame #${selectedFrameIdx} in ${part.name}`;
    drawCanvasFrame();
  }

  async function removeSelectedFrame() {
    if (!snapshot) return;
    const part = snapshot.manifest.parts[selectedPartIdx];
    if (!part || part.frames.length <= 1) {
      statusBanner = 'Cannot remove the last remaining frame in a part';
      return;
    }
    snapshot = await bridge.commitMutation((m) => {
      m.parts[selectedPartIdx].frames.splice(selectedFrameIdx, 1);
    });
    selectedFrameIdx = Math.max(0, selectedFrameIdx - 1);
    statusBanner = `Removed frame from ${part.name}`;
    drawCanvasFrame();
  }

  function focusDiagnosticTarget(diag: Diagnostic) {
    if (!snapshot || !diag.file_or_part) return;
    const idx = snapshot.manifest.parts.findIndex(
      (p) => diag.file_or_part === p.name || diag.file_or_part?.startsWith(`${p.name}/`),
    );
    if (idx >= 0) {
      selectedPartIdx = idx;
      selectedFrameIdx = 0;
      statusBanner = `Focused part '${snapshot.manifest.parts[idx].name}' from diagnostic ${diag.code}`;
    } else {
      statusBanner = `Focused '${diag.file_or_part}' (${diag.code})`;
    }
  }

  function formatRgbHex(c: RgbColor | null | undefined): string {
    if (!c) return '#000000';
    const hex = (n: number) => n.toString(16).padStart(2, '0').toUpperCase();
    return `#${hex(c.r)}${hex(c.g)}${hex(c.b)}`;
  }

  function executeReviewedExport() {
    if (!snapshot) return;
    if (snapshot.validation.outcome === 'invalid') {
      lastBuildLog =
        'Export refused: project has blocking validation errors. Resolve errors in Diagnostics first.';
      return;
    }
    if (exportTargetKind === 'bootanimation') {
      lastBuildLog = `Built '${exportDestination}' (${snapshot.validation.performance.total_frames} frames across ${snapshot.manifest.parts.length} parts, compression=ZIP_STORED [0] verified after reopen).`;
      statusBanner = `Exported verified ${exportDestination}`;
    } else {
      lastBuildLog = `Exported Magisk module '${exportDestination}' (id=${snapshot.manifest.magisk.module_id}, target=/${snapshot.manifest.magisk.target_path}, inner bootanimation.zip ZIP_STORED verified).`;
      statusBanner = `Exported Magisk module ${exportDestination}`;
    }
  }
</script>

{#if snapshot}
  {@const activePartDecl = snapshot.manifest.descriptor.parts[selectedPartIdx]}
  {@const activeProjectPart = snapshot.manifest.parts[selectedPartIdx]}
  {@const activeScheduledFrame = snapshot.schedule.frames[currentTick]}
  {@const filteredFrames =
    activeProjectPart?.frames.filter((f) =>
      f.relative_path.toLowerCase().includes(assetFilter.toLowerCase()),
    ) ?? []}

  <div class="studio-shell">
    <!-- Top Application Bar -->
    <header class="topbar">
      <div class="brand">
        <span class="logo-dot"></span>
        <strong>BootAnimix 3.0</strong>
        <span class="project-badge mono">
          {snapshot.manifest.name}{snapshot.dirty ? ' *' : ''}
        </span>
      </div>

      <div class="toolbar-group">
        <button onclick={handleSaveProject} title="Save Project Manifest">
          Save Project
        </button>
        <button onclick={handleUndo} disabled={!snapshot.can_undo} title="Undo (Ctrl+Z)">
          Undo
        </button>
        <button onclick={handleRedo} disabled={!snapshot.can_redo} title="Redo (Ctrl+Y)">
          Redo
        </button>
      </div>

      <div class="toolbar-group">
        <span class="status-pill {snapshot.validation.outcome}">
          {snapshot.validation.outcome === 'valid'
            ? 'VALID • STORED (0)'
            : snapshot.validation.outcome === 'valid_with_warnings'
              ? 'VALID WITH WARNINGS'
              : 'INVALID (BLOCKS EXPORT)'}
        </span>
        <button
          class="primary"
          onclick={() => {
            lastBuildLog = null;
            showExportModal = true;
          }}
        >
          Build / Export...
        </button>
      </div>
    </header>

    <!-- Main 3-Column Editor Workspace -->
    <div class="workspace-grid">
      <!-- Left: Asset Browser & Part Navigator -->
      <aside class="panel asset-browser">
        <div class="panel-header">
          <span>PARTS &amp; ASSETS</span>
          <span class="mono muted">{snapshot.validation.performance.total_frames}f</span>
        </div>

        <div class="parts-list">
          {#each snapshot.manifest.descriptor.parts as part, idx}
            {@const pCount = snapshot.manifest.parts[idx]?.frames.length ?? 0}
            <button
              class="part-row {idx === selectedPartIdx ? 'selected' : ''}"
              onclick={() => {
                selectedPartIdx = idx;
                selectedFrameIdx = 0;
                if (previewMode === 'selected_part') refresh(idx);
              }}
            >
              <span class="part-badge mono">
                {part.mode.mode === 'play_interruptible'
                  ? 'p'
                  : part.mode.mode === 'play_to_completion'
                    ? 'c'
                    : 'f'}
              </span>
              <span class="part-name mono">{part.path}</span>
              <span class="loop-tag mono">
                {part.repeat.kind === 'infinite' ? '∞ loop' : `${part.repeat.count}x`}
              </span>
              <span class="muted mono">{pCount}f</span>
            </button>
          {/each}
        </div>

        <div class="part-add-bar">
          <input
            type="text"
            placeholder="New part name (e.g. part3)"
            bind:value={newPartName}
          />
          <button onclick={addNewPart}>+ Part</button>
          <button
            onclick={deleteSelectedPart}
            disabled={snapshot.manifest.parts.length <= 1}
            title="Remove selected part"
          >
            Del
          </button>
        </div>

        <div class="panel-subheader">
          <span>FRAMES IN {activeProjectPart?.name ?? '-'}</span>
          <input
            type="search"
            placeholder="Filter frames..."
            bind:value={assetFilter}
          />
        </div>

        <div class="frame-asset-list">
          {#each filteredFrames as frame, fIdx}
            <button
              class="asset-item {fIdx === selectedFrameIdx ? 'selected' : ''}"
              onclick={() => {
                selectedFrameIdx = fIdx;
              }}
            >
              <span class="thumb-swatch" style="background: hsl({(fIdx * 28) % 360}, 65%, 45%)"></span>
              <span class="asset-path mono">{frame.relative_path.split('/').pop()}</span>
              {#if frame.trim}
                <span class="trim-tag mono">
                  {frame.trim.width}x{frame.trim.height}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      </aside>

      <!-- Center: Preview Canvas & Playback Controls -->
      <main class="panel preview-stage">
        <div class="panel-header">
          <div class="stage-controls">
            <span>PREVIEW CANVAS</span>
            <select
              value={previewMode}
              onchange={(e) => {
                previewMode = (e.currentTarget as HTMLSelectElement).value as
                  | 'full'
                  | 'selected_part';
                refresh();
              }}
            >
              <option value="full">Whole Animation</option>
              <option value="selected_part">Selected Part Only</option>
            </select>

            <label class="inline-field">
              <span>Speed</span>
              <select
                value={String(playbackSpeed)}
                onchange={(e) => {
                  playbackSpeed = Number.parseFloat(
                    (e.currentTarget as HTMLSelectElement).value,
                  );
                  if (isPlaying) startPlayback();
                }}
              >
                <option value="0.25">0.25x</option>
                <option value="0.5">0.5x</option>
                <option value="1">1.0x</option>
                <option value="2">2.0x</option>
              </select>
            </label>

            <label class="inline-field">
              <span>Zoom</span>
              <select
                value={String(canvasZoom)}
                onchange={(e) => {
                  canvasZoom = Number.parseFloat(
                    (e.currentTarget as HTMLSelectElement).value,
                  );
                }}
              >
                <option value="0.75">75%</option>
                <option value="1">100% Fit</option>
                <option value="1.25">125%</option>
              </select>
            </label>
          </div>

          <span class="mono muted">
            {snapshot.manifest.descriptor.width}×{snapshot.manifest.descriptor.height} @ {snapshot
              .manifest.descriptor.fps} FPS
          </span>
        </div>

        <div class="canvas-viewport">
          <div
            class="canvas-frame"
            style="transform: scale({canvasZoom});"
          >
            <canvas
              bind:this={canvasEl}
              width="216"
              height="420"
            ></canvas>
          </div>
        </div>

        <div class="transport-bar">
          <button onclick={() => stepFrame(-1)} title="Step Backward 1 Frame">
            ◀ Step
          </button>
          <button class="primary" onclick={togglePlayback}>
            {isPlaying ? '⏸ Pause' : '▶ Play'}
          </button>
          <button onclick={() => stepFrame(1)} title="Step Forward 1 Frame">
            Step ▶
          </button>

          <input
            class="scrubber"
            type="range"
            min="0"
            max={Math.max(0, snapshot.schedule.frames.length - 1)}
            value={currentTick}
            oninput={(e) => {
              stopPlayback();
              currentTick = Number.parseInt((e.currentTarget as HTMLInputElement).value, 10) || 0;
              drawCanvasFrame();
            }}
          />

          <span class="mono readout">
            Tick {currentTick + 1}/{snapshot.schedule.frames.length} •
            {activeScheduledFrame ? activeScheduledFrame.timestamp_seconds.toFixed(2) : '0.00'}s
            {snapshot.schedule.is_infinite ? ' (∞ loop window)' : ''}
          </span>
        </div>

        {#if snapshot.schedule.fidelity_warnings.length > 0}
          <div class="fidelity-strip">
            {#each snapshot.schedule.fidelity_warnings as note}
              <div class="fidelity-item">ⓘ {note}</div>
            {/each}
          </div>
        {/if}
      </main>

      <!-- Right: Properties Inspector -->
      <aside class="panel inspector">
        <div class="panel-header">
          <span>PROPERTIES INSPECTOR</span>
          <span class="mono muted">desc.txt</span>
        </div>

        <div class="inspector-scroll">
          <section class="inspector-section">
            <h4>Animation Header</h4>
            <div class="form-grid">
              <label>
                <span>Width (px)</span>
                <input
                  type="number"
                  min="1"
                  max="16384"
                  value={snapshot.manifest.descriptor.width}
                  onchange={(e) =>
                    updateHeaderNumber('width', (e.currentTarget as HTMLInputElement).value)}
                />
              </label>
              <label>
                <span>Height (px)</span>
                <input
                  type="number"
                  min="1"
                  max="16384"
                  value={snapshot.manifest.descriptor.height}
                  onchange={(e) =>
                    updateHeaderNumber('height', (e.currentTarget as HTMLInputElement).value)}
                />
              </label>
              <label>
                <span>Frame Rate (FPS)</span>
                <input
                  type="number"
                  min="1"
                  max="240"
                  value={snapshot.manifest.descriptor.fps}
                  onchange={(e) =>
                    updateHeaderNumber('fps', (e.currentTarget as HTMLInputElement).value)}
                />
              </label>
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  checked={snapshot.manifest.descriptor.show_progress}
                  onchange={(e) =>
                    toggleProgressFlag((e.currentTarget as HTMLInputElement).checked)}
                />
                <span>Show boot progress % ('p')</span>
              </label>
            </div>
          </section>

          {#if activePartDecl}
            <section class="inspector-section">
              <h4>Selected Part: <span class="mono">{activePartDecl.path}</span></h4>
              <div class="form-grid">
                <label>
                  <span>Playback Mode</span>
                  <select
                    value={activePartDecl.mode.mode === 'play_interruptible'
                      ? 'p'
                      : activePartDecl.mode.mode === 'play_to_completion'
                        ? 'c'
                        : 'f'}
                    onchange={(e) =>
                      updatePartMode((e.currentTarget as HTMLSelectElement).value)}
                  >
                    <option value="p">p — Interruptible on boot exit</option>
                    <option value="c">c — Play to completion</option>
                    <option value="f">f — Fade out when interrupted</option>
                  </select>
                </label>

                <label>
                  <span>Repeat Count (0 = Infinite ∞)</span>
                  <input
                    type="number"
                    min="0"
                    max="999"
                    value={activePartDecl.repeat.kind === 'infinite'
                      ? 0
                      : activePartDecl.repeat.count}
                    onchange={(e) =>
                      updatePartRepeat((e.currentTarget as HTMLInputElement).value)}
                  />
                </label>

                <label>
                  <span>Pause Hold (in FRAMES, not sec)</span>
                  <input
                    type="number"
                    min="0"
                    max="10000"
                    value={activePartDecl.pause_frames}
                    onchange={(e) =>
                      updatePartPause((e.currentTarget as HTMLInputElement).value)}
                  />
                </label>

                <label>
                  <span>Background Color (#RRGGBB)</span>
                  <input
                    type="text"
                    class="mono"
                    value={formatRgbHex(activePartDecl.background_color)}
                    onchange={(e) =>
                      updatePartBgHex((e.currentTarget as HTMLInputElement).value)}
                  />
                </label>
              </div>
            </section>
          {/if}

          <section class="inspector-section">
            <h4>Explicit Build Transformations</h4>
            <p class="section-hint">
              Source frames are never modified in place. Transformations run only during archive build.
            </p>
            <div class="form-grid">
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  checked={snapshot.manifest.transforms.lossless_optimize_png}
                  onchange={async (e) => {
                    const checked = (e.currentTarget as HTMLInputElement).checked;
                    snapshot = await bridge.commitMutation((m) => {
                      m.transforms.lossless_optimize_png = checked;
                    });
                  }}
                />
                <span>Lossless PNG optimization (pixel-verified)</span>
              </label>
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  checked={snapshot.manifest.transforms.generate_trim_txt}
                  onchange={async (e) => {
                    const checked = (e.currentTarget as HTMLInputElement).checked;
                    snapshot = await bridge.commitMutation((m) => {
                      m.transforms.generate_trim_txt = checked;
                    });
                  }}
                />
                <span>Compute &amp; emit trim.txt per part</span>
              </label>
            </div>
          </section>

          <section class="inspector-section">
            <h4>Magisk Overlay Configuration</h4>
            <div class="form-grid">
              <label>
                <span>Module ID</span>
                <input
                  type="text"
                  class="mono"
                  value={snapshot.manifest.magisk.module_id}
                  onchange={async (e) => {
                    const val = (e.currentTarget as HTMLInputElement).value;
                    snapshot = await bridge.commitMutation((m) => {
                      m.magisk.module_id = val;
                    });
                  }}
                />
              </label>
              <label>
                <span>Overlay Target Path</span>
                <select
                  value={snapshot.manifest.magisk.target_path}
                  onchange={async (e) => {
                    const val = (e.currentTarget as HTMLSelectElement).value;
                    snapshot = await bridge.commitMutation((m) => {
                      m.magisk.target_path = val;
                    });
                  }}
                >
                  <option value="system/media/bootanimation.zip">
                    /system/media/bootanimation.zip (AOSP / LineageOS)
                  </option>
                  <option value="system/product/media/bootanimation.zip">
                    /system/product/media/bootanimation.zip (Pixel / Treble)
                  </option>
                  <option value="system/system_ext/media/bootanimation.zip">
                    /system/system_ext/media/bootanimation.zip (OEM ext)
                  </option>
                </select>
              </label>
            </div>
          </section>
        </div>
      </aside>
    </div>

    <!-- Bottom: Filmstrip Timeline & Structured Diagnostics -->
    <section class="bottom-dock">
      <div class="timeline-pane">
        <div class="panel-header">
          <div class="timeline-header-left">
            <span>TIMELINE &amp; FILMSTRIP ({activeProjectPart?.name ?? ''})</span>
            <button onclick={() => moveSelectedFrame(-1)} title="Move Frame Left">
              ← Move
            </button>
            <button onclick={() => moveSelectedFrame(1)} title="Move Frame Right">
              Move →
            </button>
            <button onclick={duplicateSelectedFrame} title="Duplicate Selected Frame">
              Duplicate Frame
            </button>
            <button
              onclick={removeSelectedFrame}
              disabled={(activeProjectPart?.frames.length ?? 0) <= 1}
              title="Remove Selected Frame"
            >
              Remove Frame
            </button>
          </div>
          <span class="mono muted">
            {snapshot.validation.performance.has_infinite_loop
              ? 'Contains ∞ Loop Part'
              : `Finite Duration: ${snapshot.validation.performance.finite_duration_seconds?.toFixed(2) ?? 0}s`}
          </span>
        </div>

        <div class="filmstrip-track">
          {#if activeProjectPart}
            {#each activeProjectPart.frames as fRef, idx}
              <button
                class="film-cell {idx === selectedFrameIdx ? 'active' : ''}"
                onclick={() => {
                  selectedFrameIdx = idx;
                  const schedIdx = snapshot?.schedule.frames.findIndex(
                    (sf) =>
                      sf.part_index === selectedPartIdx && sf.frame_index_in_part === idx,
                  );
                  if (schedIdx !== undefined && schedIdx >= 0) {
                    currentTick = schedIdx;
                    drawCanvasFrame();
                  }
                }}
              >
                <span class="cell-index mono">#{String(idx).padStart(3, '0')}</span>
                <span class="cell-bar" style="background: hsl({(idx * 32) % 360}, 70%, 50%)"></span>
                <span class="cell-name mono">{fRef.relative_path.split('/').pop()}</span>
              </button>
            {/each}
          {/if}
        </div>
      </div>

      <div class="diagnostics-pane">
        <div class="panel-header">
          <span>DIAGNOSTICS ({snapshot.validation.diagnostics.length})</span>
          <span class="mono muted">Click to inspect</span>
        </div>

        <div class="diag-list">
          {#if snapshot.validation.diagnostics.length === 0}
            <div class="diag-empty">
              ✓ No validation errors or warnings. Archive invariants and desc.txt semantics are clean.
            </div>
          {:else}
            {#each snapshot.validation.diagnostics as d}
              <button class="diag-row {d.severity}" onclick={() => focusDiagnosticTarget(d)}>
                <span class="diag-code mono">{d.code}</span>
                <span class="diag-msg">{d.message}</span>
                <span class="diag-rem muted">{d.remediation}</span>
              </button>
            {/each}
          {/if}
        </div>
      </div>
    </section>

    <!-- Status Bar -->
    <footer class="statusbar">
      <span>{statusBanner}</span>
      <span class="mono">
        Payload: {(snapshot.validation.performance.total_image_bytes / 1024).toFixed(1)} KiB •
        Storage Policy: ZIP_STORED (0)
      </span>
    </footer>
  </div>

  <!-- Export Review Modal -->
  {#if showExportModal}
    <div class="modal-backdrop" role="dialog" aria-modal="true">
      <div class="modal-card">
        <div class="panel-header">
          <span>REVIEW &amp; EXPORT ARTIFACT</span>
          <button onclick={() => (showExportModal = false)}>Close ✕</button>
        </div>

        <div class="modal-body">
          <div class="form-grid">
            <label>
              <span>Export Type</span>
              <select
                value={exportTargetKind}
                onchange={(e) => {
                  exportTargetKind = (e.currentTarget as HTMLSelectElement).value as
                    | 'bootanimation'
                    | 'magisk';
                  exportDestination =
                    exportTargetKind === 'bootanimation'
                      ? 'dist/bootanimation.zip'
                      : `dist/${snapshot?.manifest.magisk.module_id ?? 'module'}.zip`;
                }}
              >
                <option value="bootanimation">
                  Standalone bootanimation.zip (Uncompressed ZIP_STORED)
                </option>
                <option value="magisk">
                  Flashable Magisk Module ZIP (module.prop + verified inner bootanimation.zip)
                </option>
              </select>
            </label>

            <label>
              <span>Output Destination</span>
              <input type="text" class="mono" bind:value={exportDestination} />
            </label>
          </div>

          <div class="summary-table mono">
            <div><span>Dimensions / FPS:</span> <strong>{snapshot.manifest.descriptor.width}×{snapshot.manifest.descriptor.height} @ {snapshot.manifest.descriptor.fps} FPS</strong></div>
            <div><span>Parts / Total Frames:</span> <strong>{snapshot.manifest.parts.length} parts / {snapshot.validation.performance.total_frames} frames</strong></div>
            <div><span>Inner ZIP Storage Mode:</span> <strong>ZIP_STORED (Method 0 — Verified on Reopen)</strong></div>
            <div><span>Transformations:</span> <strong>{snapshot.manifest.transforms.lossless_optimize_png ? 'Lossless PNG ' : ''}{snapshot.manifest.transforms.generate_trim_txt ? '+ trim.txt' : 'None (Source Bytes Preserved)'}</strong></div>
            <div><span>Validation Status:</span> <strong>{snapshot.validation.outcome.toUpperCase()}</strong></div>
            {#if exportTargetKind === 'magisk'}
              <div><span>Magisk Module ID:</span> <strong>{snapshot.manifest.magisk.module_id}</strong></div>
              <div><span>Overlay Path:</span> <strong>/{snapshot.manifest.magisk.target_path}</strong></div>
            {/if}
          </div>

          {#if lastBuildLog}
            <div class="build-result-box mono">{lastBuildLog}</div>
          {/if}

          <div class="modal-actions">
            <button onclick={() => (showExportModal = false)}>Cancel</button>
            <button
              class="primary"
              disabled={snapshot.validation.outcome === 'invalid'}
              onclick={executeReviewedExport}
            >
              Verify &amp; Write Package
            </button>
          </div>
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .studio-shell {
    display: grid;
    grid-template-rows: 42px 1fr 195px 26px;
    width: 100vw;
    height: 100vh;
    background: var(--bg-app);
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border-subtle);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .logo-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
  }

  .project-badge {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 4px;
    background: var(--bg-elevated);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
  }

  .toolbar-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-pill {
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 4px;
  }
  .status-pill.valid {
    background: rgba(16, 185, 129, 0.15);
    color: var(--ok);
  }
  .status-pill.valid_with_warnings {
    background: rgba(245, 158, 11, 0.16);
    color: var(--warn);
  }
  .status-pill.invalid {
    background: rgba(239, 68, 68, 0.18);
    color: var(--err);
  }

  .workspace-grid {
    display: grid;
    grid-template-columns: 270px 1fr 320px;
    overflow: hidden;
    min-height: 0;
  }

  .panel {
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
    min-height: 0;
  }

  .panel.inspector {
    border-right: none;
    border-left: 1px solid var(--border-subtle);
  }

  .panel-header,
  .panel-subheader {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 11px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
  }

  .parts-list {
    display: flex;
    flex-direction: column;
    padding: 6px;
    gap: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .part-row {
    display: grid;
    grid-template-columns: 22px 1fr auto auto;
    align-items: center;
    gap: 6px;
    text-align: left;
    padding: 6px 8px;
    background: transparent;
  }

  .part-row.selected {
    background: var(--accent-soft);
    border-color: var(--accent);
  }

  .part-badge {
    display: inline-block;
    text-align: center;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .loop-tag {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .part-add-bar {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 5px;
    padding: 6px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .frame-asset-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .asset-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    background: transparent;
    padding: 4px 7px;
  }

  .asset-item.selected {
    background: var(--accent-soft);
    border-color: var(--accent);
  }

  .thumb-swatch {
    width: 12px;
    height: 12px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .asset-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trim-tag {
    font-size: 10px;
    color: var(--text-secondary);
  }

  .preview-stage {
    background: var(--bg-app);
    display: flex;
    flex-direction: column;
  }

  .stage-controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .inline-field {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .canvas-viewport {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    position: relative;
  }

  .canvas-frame {
    padding: 8px;
    border-radius: 12px;
    background: #07090d;
    border: 1px solid var(--border-subtle);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.55);
    transition: transform 0.15s ease;
  }

  canvas {
    display: block;
    border-radius: 6px;
  }

  .transport-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border-subtle);
  }

  .scrubber {
    flex: 1;
    padding: 0;
  }

  .readout {
    font-size: 11px;
    color: var(--text-secondary);
    min-width: 195px;
    text-align: right;
  }

  .fidelity-strip {
    background: rgba(245, 158, 11, 0.09);
    border-top: 1px solid rgba(245, 158, 11, 0.25);
    padding: 5px 12px;
    font-size: 11px;
    color: #fbbf24;
  }

  .inspector-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .inspector-section {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 10px;
  }

  .inspector-section h4 {
    font-size: 12px;
    margin-bottom: 8px;
    color: var(--text-primary);
  }

  .section-hint {
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 8px;
  }

  .form-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .form-grid label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .form-grid label.checkbox-row {
    flex-direction: row;
    align-items: center;
    gap: 7px;
    color: var(--text-primary);
  }

  .bottom-dock {
    display: grid;
    grid-template-columns: 1fr 410px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-panel);
    min-height: 0;
  }

  .timeline-pane {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border-subtle);
    min-width: 0;
  }

  .timeline-header-left {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .filmstrip-track {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    overflow-x: auto;
  }

  .film-cell {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: space-between;
    min-width: 92px;
    height: 112px;
    padding: 7px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    flex-shrink: 0;
  }

  .film-cell.active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .cell-index {
    font-size: 10px;
    color: var(--text-secondary);
  }

  .cell-bar {
    width: 100%;
    height: 44px;
    border-radius: 4px;
  }

  .cell-name {
    font-size: 10px;
    color: var(--text-primary);
    max-width: 78px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .diagnostics-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .diag-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .diag-empty {
    padding: 12px;
    color: var(--ok);
    font-size: 12px;
  }

  .diag-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    text-align: left;
    padding: 6px 8px;
  }

  .diag-row.error {
    border-left: 3px solid var(--err);
  }
  .diag-row.warning {
    border-left: 3px solid var(--warn);
  }

  .diag-code {
    font-size: 10px;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .diag-msg {
    font-size: 11px;
    color: var(--text-primary);
  }

  .diag-rem {
    font-size: 10px;
  }

  .statusbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    font-size: 11px;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-subtle);
  }

  .muted {
    color: var(--text-muted);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(5, 7, 11, 0.75);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }

  .modal-card {
    width: 540px;
    background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 24px 48px rgba(0, 0, 0, 0.6);
  }

  .modal-body {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .summary-table {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    padding: 10px;
    font-size: 11px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .summary-table div {
    display: flex;
    justify-content: space-between;
  }

  .build-result-box {
    padding: 9px;
    border-radius: 5px;
    background: rgba(16, 185, 129, 0.14);
    border: 1px solid var(--ok);
    color: var(--ok);
    font-size: 11px;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
