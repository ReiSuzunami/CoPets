import "pixi.js/unsafe-eval";
import { Application, Graphics, Rectangle, Sprite, Texture } from "pixi.js";
import {
  ANIMATIONS,
  advanceAnimationFrame,
  animationForDragDirection,
  animationForState,
  frameForMotionPreference,
  isTerminalState,
  shouldAdvanceAnimation,
} from "./pet.js";

export class PixiPet {
  constructor(container, { reducedMotion = false } = {}) {
    this.container = container;
    this.app = new Application();
    this.shadow = new Graphics();
    this.shadow.eventMode = "none";
    this.shadow.visible = false;
    this.sprite = new Sprite();
    this.atlas = null;
    this.textures = new Map();
    this.state = "idle";
    this.animation = "idle";
    this.frame = 0;
    this.elapsed = 0;
    this.playOnce = false;
    this.resting = false;
    this.lookTexture = null;
    this.lookTextures = [];
    this.reducedMotion = Boolean(reducedMotion);
  }

  async init() {
    await this.app.init({
      resizeTo: this.container,
      backgroundAlpha: 0,
      antialias: false,
      autoDensity: true,
      resolution: Math.max(1, window.devicePixelRatio || 1),
    });
    this.app.canvas.setAttribute("aria-label", "Animated Codex pet");
    this.container.appendChild(this.app.canvas);
    this.sprite.anchor.set(0.5, 1);
    this.app.stage.addChild(this.shadow);
    this.app.stage.addChild(this.sprite);
    this.app.ticker.add((ticker) => this.tick(ticker.deltaMS));
    this.layout();
    window.addEventListener("resize", this.onResize);
    this.container.addEventListener("pointermove", this.onPointerMove);
    this.container.addEventListener("pointerleave", this.onPointerLeave);
  }

  onResize = () => this.layout();

  async load(pet, isCurrent = () => true) {
    const image = new Image();
    image.src = pet.spritesheetDataUrl;
    await image.decode();
    if (!isCurrent()) return false;

    const atlas = Texture.from(image);
    const textures = new Map();
    const lookTextures = [];
    for (const [name, spec] of Object.entries(ANIMATIONS)) {
      const frames = [];
      for (let column = 0; column < spec.durations.length; column += 1) {
        frames.push(new Texture({
          source: atlas.source,
          frame: new Rectangle(column * pet.cellWidth, spec.row * pet.cellHeight, pet.cellWidth, pet.cellHeight),
        }));
      }
      textures.set(name, frames);
    }
    if (pet.spriteVersionNumber === 2) {
      for (let index = 0; index < 16; index += 1) {
        const row = 9 + Math.floor(index / 8);
        const column = index % 8;
        lookTextures.push(new Texture({
          source: atlas.source,
          frame: new Rectangle(column * pet.cellWidth, row * pet.cellHeight, pet.cellWidth, pet.cellHeight),
        }));
      }
    }
    if (!isCurrent()) {
      this.destroyTextureSet(textures, lookTextures, atlas);
      return false;
    }

    const previousTextures = this.textures;
    const previousLookTextures = this.lookTextures;
    const previousAtlas = this.atlas;
    this.textures = textures;
    this.lookTextures = lookTextures;
    this.atlas = atlas;
    this.cellWidth = pet.cellWidth;
    this.cellHeight = pet.cellHeight;
    this.lookTexture = null;
    this.frame = 0;
    this.elapsed = 0;
    this.drawShadow();
    this.shadow.visible = true;
    this.sprite.visible = true;
    this.applyFrame();
    this.layout();
    this.destroyTextureSet(previousTextures, previousLookTextures, previousAtlas);
    return true;
  }

  destroyTextureSet(textures, lookTextures, atlas) {
    for (const frames of textures.values()) for (const texture of frames) texture.destroy();
    for (const texture of lookTextures) texture.destroy();
    atlas?.destroy(true);
  }

  clear() {
    const previousTextures = this.textures;
    const previousLookTextures = this.lookTextures;
    const previousAtlas = this.atlas;
    this.textures = new Map();
    this.lookTextures = [];
    this.atlas = null;
    this.cellWidth = undefined;
    this.cellHeight = undefined;
    this.lookTexture = null;
    this.frame = 0;
    this.elapsed = 0;
    this.shadow.clear();
    this.shadow.visible = false;
    this.sprite.visible = false;
    this.sprite.texture = Texture.EMPTY;
    this.destroyTextureSet(previousTextures, previousLookTextures, previousAtlas);
  }

  setState(state) {
    if (state === this.state) return;
    this.state = state;
    const next = animationForState(state);
    this.animation = next;
    this.playOnce = isTerminalState(state);
    this.resting = false;
    this.lookTexture = null;
    this.frame = 0;
    this.elapsed = 0;
    this.applyFrame();
  }

  setDragDirection(direction) {
    const next = animationForDragDirection(direction);
    if (this.animation === next && !this.resting) return;
    this.animation = next;
    this.playOnce = false;
    this.resting = false;
    this.lookTexture = null;
    this.frame = 0;
    this.elapsed = 0;
    this.applyFrame();
  }

  restoreStateAnimation() {
    const terminal = isTerminalState(this.state);
    this.animation = terminal ? "idle" : animationForState(this.state);
    this.playOnce = false;
    this.resting = terminal;
    this.lookTexture = null;
    this.frame = 0;
    this.elapsed = 0;
    this.applyFrame();
  }

  setReducedMotion(enabled) {
    const next = Boolean(enabled);
    if (next === this.reducedMotion) return;
    this.reducedMotion = next;
    this.frame = 0;
    this.elapsed = 0;
    this.applyFrame();
  }

  tick(deltaMs) {
    if (!shouldAdvanceAnimation({
      reducedMotion: this.reducedMotion,
      resting: this.resting,
      hasTextures: this.textures.size > 0,
    })) return;
    const spec = ANIMATIONS[this.animation];
    this.elapsed += deltaMs;
    if (this.elapsed < spec.durations[this.frame]) return;
    this.elapsed = 0;
    const next = advanceAnimationFrame(this.animation, this.frame, this.playOnce);
    this.animation = next.animation;
    this.frame = next.frame;
    this.resting = next.resting;
    this.applyFrame();
  }

  applyFrame() {
    if (this.lookTexture && this.animation === "idle") {
      this.sprite.texture = this.lookTexture;
      return;
    }
    const frames = this.textures.get(this.animation);
    if (frames?.length) {
      this.sprite.texture = frames[frameForMotionPreference(this.frame, this.reducedMotion)];
    }
  }

  onPointerMove = (event) => {
    if (this.animation !== "idle" || this.lookTextures.length !== 16) return;
    const bounds = this.container.getBoundingClientRect();
    const dx = event.clientX - bounds.left - bounds.width / 2;
    const dy = event.clientY - bounds.top - bounds.height / 2;
    if (Math.hypot(dx, dy) < 34) {
      this.lookTexture = null;
    } else {
      const degrees = (Math.atan2(dx, -dy) * 180 / Math.PI + 360) % 360;
      this.lookTexture = this.lookTextures[Math.round(degrees / 22.5) % 16];
    }
    this.applyFrame();
  };

  onPointerLeave = () => {
    this.lookTexture = null;
    this.applyFrame();
  };

  drawShadow() {
    if (!this.cellWidth || !this.cellHeight) return;
    this.shadow
      .clear()
      .ellipse(0, 0, this.cellWidth * 0.34, this.cellHeight * 0.045)
      .fill({ color: 0x000000, alpha: 0.16 });
  }

  layout() {
    if (!this.cellWidth || !this.cellHeight) return;
    const width = this.app.screen.width;
    const height = this.app.screen.height;
    const scale = Math.min((width - 20) / this.cellWidth, (height - 34) / this.cellHeight);
    this.sprite.scale.set(scale);
    this.sprite.position.set(width / 2, height - 10);
    this.shadow.scale.set(scale);
    this.shadow.position.set(width / 2, height - 10 - this.cellHeight * scale * 0.06);
  }

  destroy() {
    window.removeEventListener("resize", this.onResize);
    this.container.removeEventListener("pointermove", this.onPointerMove);
    this.container.removeEventListener("pointerleave", this.onPointerLeave);
    this.clear();
    this.app.destroy(true);
  }
}
