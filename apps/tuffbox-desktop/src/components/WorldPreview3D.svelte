<script lang="ts">
  /**
   * 3D preview of a world's generated chunks.
   *
   * Deliberately kept off the startup path: `three` is imported here and this
   * component is only ever reached through a dynamic `import()` from World.svelte,
   * so none of it lands in the bundle `check-bundle-budget.mjs` measures.
   *
   * Data comes from `read_world_column` (Rust) — palette + unpacked indices —
   * and the mesher/colour/clone layers above are framework-free and unit-tested.
   */
  import * as THREE from "three";
  import { RotateCcw, X } from "@lucide/svelte";
  import { api } from "../lib/api";
  import { projectPath } from "../lib/store";
  import { BlockColorResolver, type TextureColorSource } from "../lib/world/blockColors";
  import { ChunkStore } from "../lib/world/chunkStore";
  import { columnBlockAt, meshColumn } from "../lib/world/mesher";

  let {
    worldName,
    dimension = "overworld",
    onclose,
  }: { worldName: string; dimension?: string; onclose?: () => void } = $props();

  /** Chunks loaded in every direction around the camera target. */
  const VIEW_RADIUS = 6;
  const BLOCKS_PER_CHUNK = 16;

  let host = $state<HTMLDivElement | null>(null);
  let status = $state("Starting…");
  let drawnChunks = $state(0);
  let drawnFaces = $state(0);

  /**
   * Bridge from markup to the live camera, which only exists inside the effect.
   * Deliberately not `$state`: three.js objects must never be proxied.
   */
  const view = { reset: () => {} };

  /**
   * Mean colour of a block texture.
   *
   * `ponytail:` drawn into a 1×1 canvas instead of walking every pixel — a
   * 16×16 block texture downscaled with smoothing is already an average, and the
   * result is only used as a flat tint.
   */
  async function averageTextureColor(dataUrl: string): Promise<number | null> {
    if (typeof createImageBitmap !== "function" || typeof OffscreenCanvas !== "function") return null;
    try {
      const blob = await (await fetch(dataUrl)).blob();
      const bitmap = await createImageBitmap(blob);
      const canvas = new OffscreenCanvas(1, 1);
      const context = canvas.getContext("2d");
      if (!context) return null;
      context.drawImage(bitmap, 0, 0, 1, 1);
      bitmap.close();
      const [r, g, b] = context.getImageData(0, 0, 1, 1).data;
      return (r << 16) | (g << 8) | b;
    } catch {
      return null;
    }
  }

  $effect(() => {
    // Reading these keeps the effect honest: switching world or layer rebuilds.
    const world = worldName;
    const layer = dimension;
    const element = host;
    if (!element) return;

    const manifestPath = $projectPath ?? undefined;
    const store = new ChunkStore((x, z) => api.worlds.readColumn(world, x, z, layer, manifestPath));
    const resolver = new BlockColorResolver({
      fetch: (names) => api.recipes.itemIconsBatch(names, manifestPath),
      average: averageTextureColor,
    } satisfies TextureColorSource);

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    element.appendChild(renderer.domElement);
    renderer.domElement.className = "world-canvas";

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0d1117);
    const camera = new THREE.PerspectiveCamera(70, 1, 0.1, 4000);
    const material = new THREE.MeshLambertMaterial({ vertexColors: true });
    const world3d = new THREE.Group();
    scene.add(world3d);
    const sun = new THREE.DirectionalLight(0xffffff, 1.5);
    sun.position.set(0.5, 1, 0.25);
    scene.add(sun, new THREE.AmbientLight(0xffffff, 0.8));

    const target = new THREE.Vector3(0, 80, 0);
    let yaw = 0.6;
    let pitch = 0.9;
    let distance = 220;
    const pressed = new Set<string>();
    const meshed = new Set<string>();
    const objects = new Map<string, THREE.Mesh>();
    let colorsWarmed = false;
    let disposed = false;
    let frameHandle = 0;
    let lastFrame = performance.now();

    const blockLookup = {
      blockAt(x: number, y: number, z: number): string | null {
        const column = store.get(x >> 5, z >> 5);
        return column ? columnBlockAt(column, x & 15, y, z & 15) : null;
      },
    };

    function meshReadyColumns() {
      const pending = store.readyColumns().filter(({ x, z }) => !meshed.has(`${x}:${z}`));
      if (pending.length === 0) return;
      const palette = new Set<string>();
      for (const { x, z, column } of pending) {
        meshed.add(`${x}:${z}`);
        for (const section of column.sections) {
          for (const entry of section.blockPalette) palette.add(entry.name);
        }
        const mesh = meshColumn(column, blockLookup, (name) => resolver.colorOf(name));
        if (mesh.faceCount === 0) continue;
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute("position", new THREE.BufferAttribute(mesh.positions, 3));
        geometry.setAttribute("normal", new THREE.BufferAttribute(mesh.normals, 3));
        geometry.setAttribute("color", new THREE.BufferAttribute(mesh.colors, 3));
        geometry.setIndex(new THREE.BufferAttribute(mesh.indices, 1));
        const object = new THREE.Mesh(geometry, material);
        object.position.set(x * BLOCKS_PER_CHUNK, 0, z * BLOCKS_PER_CHUNK);
        world3d.add(object);
        objects.set(`${x}:${z}`, object);
      }

      if (!colorsWarmed) {
        colorsWarmed = true;
        void resolver.warm(palette).then((learned) => {
          if (disposed || learned === 0) return;
          // Real textures changed some tints: rebuild once, then never again.
          for (const object of objects.values()) {
            object.geometry.dispose();
            world3d.remove(object);
          }
          objects.clear();
          meshed.clear();
        });
      }
      drawnChunks = meshed.size;
    }

    function moveTarget(seconds: number) {
      const speed = 28 * seconds;
      const forward = new THREE.Vector3(Math.sin(yaw), 0, Math.cos(yaw)).multiplyScalar(-1);
      const right = new THREE.Vector3(forward.z, 0, -forward.x);
      if (pressed.has("w")) target.addScaledVector(forward, speed);
      if (pressed.has("s")) target.addScaledVector(forward, -speed);
      if (pressed.has("a")) target.addScaledVector(right, -speed);
      if (pressed.has("d")) target.addScaledVector(right, speed);
      if (pressed.has("e")) target.y += speed;
      if (pressed.has("q")) target.y -= speed;
    }

    function resize() {
      const width = element?.clientWidth || 1;
      const height = element?.clientHeight || 1;
      if (renderer.domElement.width === Math.round(width * renderer.getPixelRatio()) && renderer.domElement.height === Math.round(height * renderer.getPixelRatio())) {
        return;
      }
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    }

    function frame(now: number) {
      if (disposed) return;
      frameHandle = requestAnimationFrame(frame);
      const seconds = Math.min((now - lastFrame) / 1000, 0.1);
      lastFrame = now;

      moveTarget(seconds);
      const centerX = Math.floor(target.x / BLOCKS_PER_CHUNK);
      const centerZ = Math.floor(target.z / BLOCKS_PER_CHUNK);
      for (let dx = -VIEW_RADIUS; dx <= VIEW_RADIUS; dx++) {
        for (let dz = -VIEW_RADIUS; dz <= VIEW_RADIUS; dz++) {
          void store.ensure(centerX + dx, centerZ + dz);
        }
      }
      meshReadyColumns();

      const horizontal = Math.cos(pitch) * distance;
      camera.position.set(
        target.x + Math.sin(yaw) * horizontal,
        target.y + Math.sin(pitch) * distance,
        target.z + Math.cos(yaw) * horizontal,
      );
      camera.lookAt(target);
      resize();
      renderer.render(scene, camera);
      let faces = 0;
      for (const object of objects.values()) faces += (object.geometry.getIndex()?.count ?? 0) / 6;
      drawnFaces = faces;
    }

    // ── Interaction ────────────────────────────────────────────────────
    let dragging = false;
    let lastX = 0;
    let lastY = 0;

    const onPointerDown = (event: PointerEvent) => {
      dragging = true;
      lastX = event.clientX;
      lastY = event.clientY;
      renderer.domElement.setPointerCapture(event.pointerId);
    };
    const onPointerUp = (event: PointerEvent) => {
      dragging = false;
      if (renderer.domElement.hasPointerCapture(event.pointerId)) {
        renderer.domElement.releasePointerCapture(event.pointerId);
      }
    };
    const onPointerMove = (event: PointerEvent) => {
      if (!dragging) return;
      yaw -= (event.clientX - lastX) * 0.005;
      pitch = Math.max(0.08, Math.min(1.5, pitch + (event.clientY - lastY) * 0.005));
      lastX = event.clientX;
      lastY = event.clientY;
    };
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      distance = Math.max(16, Math.min(1200, distance * (event.deltaY > 0 ? 1.12 : 0.89)));
    };
    // Fly keys live on the window: the viewer is a full-panel mode, and a
    // focusable wrapper would need a non-interactive tabindex, which is both an
    // a11y smell and easy to lose focus from. Typing targets are left alone.
    const isTyping = (target: EventTarget | null) =>
      target instanceof HTMLElement &&
      (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
    const onKeyDown = (event: KeyboardEvent) => {
      if (isTyping(event.target)) return;
      const key = event.key.toLowerCase();
      if ("wasdqe".includes(key)) {
        pressed.add(key);
        event.preventDefault();
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (isTyping(event.target)) return;
      pressed.delete(event.key.toLowerCase());
    };

    renderer.domElement.addEventListener("pointerdown", onPointerDown);
    renderer.domElement.addEventListener("pointerup", onPointerUp);
    renderer.domElement.addEventListener("pointermove", onPointerMove);
    renderer.domElement.addEventListener("wheel", onWheel, { passive: false });
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    const unsubscribe = store.onChange(() => {
      if (!disposed) meshReadyColumns();
    });
    const observer = new ResizeObserver(() => resize());
    observer.observe(element);

    // Start over the world spawn so the first frames show generated terrain.
    const spawn = new THREE.Vector3(0, 80, 0);
    view.reset = () => {
      target.copy(spawn);
      yaw = 0.6;
      pitch = 0.9;
      distance = 220;
    };
    void api.worlds
      .readInfo(world, manifestPath)
      .then((info) => {
        if (disposed) return;
        spawn.set(info.spawnX, Math.max(info.spawnY, 64), info.spawnZ);
        view.reset();
        status = "";
      })
      .catch((error: unknown) => {
        if (!disposed) status = `Could not read the world: ${String(error)}`;
      });

    status = "Loading chunks…";
    frameHandle = requestAnimationFrame(frame);

    return () => {
      disposed = true;
      cancelAnimationFrame(frameHandle);
      unsubscribe();
      observer.disconnect();
      renderer.domElement.removeEventListener("pointerdown", onPointerDown);
      renderer.domElement.removeEventListener("pointerup", onPointerUp);
      renderer.domElement.removeEventListener("pointermove", onPointerMove);
      renderer.domElement.removeEventListener("wheel", onWheel);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      for (const object of objects.values()) object.geometry.dispose();
      material.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    };
  });
</script>

<div class="world-preview" bind:this={host} aria-label="3D preview of {worldName}">
  <div class="hud">
    <span class="title">{worldName}</span>
    <span class="stat">{drawnChunks} chunks</span>
    <span class="stat">{drawnFaces.toLocaleString()} faces</span>
    {#if status}<span class="stat status">{status}</span>{/if}
    <span class="spacer"></span>
    <span class="hint">drag to orbit · wheel to zoom · WASD/QE to fly</span>
    <button type="button" class="icon-btn" title="Reset camera to spawn" onclick={() => view.reset()}>
      <RotateCcw size={13} />
    </button>
    {#if onclose}
      <button type="button" class="icon-btn" title="Close preview" onclick={onclose}>
        <X size={13} />
      </button>
    {/if}
  </div>
</div>

<style>
  .world-preview {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 320px;
    overflow: hidden;
    background: #0d1117;
    outline: none;
  }

  .world-preview :global(.world-canvas) {
    display: block;
    width: 100%;
    height: 100%;
  }

  .hud {
    position: absolute;
    top: 8px;
    left: 8px;
    right: 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: color-mix(in srgb, var(--bg-secondary) 88%, transparent);
    color: var(--text-secondary);
    font-size: 12px;
    pointer-events: none;
  }

  .hud .title {
    color: var(--text-primary);
    font-weight: 600;
  }

  .hud .stat {
    font-variant-numeric: tabular-nums;
  }

  .hud .status {
    color: var(--accent-primary);
  }

  .spacer {
    flex: 1;
  }

  .hint {
    color: var(--text-muted);
    font-size: 12px;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    cursor: pointer;
    pointer-events: auto;
  }

  .icon-btn:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
</style>
