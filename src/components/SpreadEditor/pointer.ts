/**
 * Input helpers so the spread editor works with mouse, trackpad, touch and
 * Apple Pencil alike (iPadOS). Konva delivers mouse and touch as separate
 * event types; these helpers normalise them.
 */

import Konva from 'konva';

/** Konva event names covering mouse and touch. */
export const PRESS = 'mousedown touchstart';
export const MOVE = 'mousemove touchmove';
export const RELEASE = 'mouseup touchend touchcancel';
export const RELEASE_OR_LEAVE = 'mouseup mouseleave touchend touchcancel';
export const TAP = 'click tap';

/** Duration of a press that opens the context menu on touch screens. */
export const LONG_PRESS_MS = 500;
const LONG_PRESS_SLOP = 8;

type InputEvent = MouseEvent | TouchEvent | PointerEvent | WheelEvent;

/** True for a primary (left) mouse button press or any touch. */
export function isPrimaryPress(evt: Event | undefined): boolean {
  if (!evt) return false;
  if ('touches' in evt) return (evt as TouchEvent).touches.length <= 1;
  return (evt as MouseEvent).button === 0;
}

export function isTouch(evt: Event | undefined): boolean {
  return !!evt && 'touches' in evt;
}

export function modifier(evt: Event | undefined, key: 'shiftKey' | 'altKey' | 'metaKey' | 'ctrlKey'): boolean {
  return !!evt && !!(evt as KeyboardEvent)[key];
}

/** Viewport coordinates of a mouse or (first) touch point. */
export function clientPoint(evt: InputEvent): { x: number; y: number } {
  if ('touches' in evt) {
    const t = evt.touches[0] ?? evt.changedTouches[0];
    return t ? { x: t.clientX, y: t.clientY } : { x: 0, y: 0 };
  }
  return { x: evt.clientX, y: evt.clientY };
}

/**
 * Call `handler` when a node is pressed and held without moving (touch
 * stand-in for right-click). Returns a disposer.
 */
export function onLongPress(node: Konva.Node, handler: (point: { x: number; y: number }, target: Konva.Node) => void): () => void {
  let timer: number | null = null;
  let start = { x: 0, y: 0 };
  const cancel = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  };
  const begin = (e: Konva.KonvaEventObject<TouchEvent>) => {
    if (e.evt.touches.length !== 1) return cancel();
    start = clientPoint(e.evt);
    const target = e.target;
    cancel();
    timer = window.setTimeout(() => {
      timer = null;
      handler(start, target);
    }, LONG_PRESS_MS);
  };
  const move = (e: Konva.KonvaEventObject<TouchEvent>) => {
    const p = clientPoint(e.evt);
    if (Math.hypot(p.x - start.x, p.y - start.y) > LONG_PRESS_SLOP) cancel();
  };
  node.on('touchstart.longpress', begin);
  node.on('touchmove.longpress', move);
  node.on('touchend.longpress touchcancel.longpress dragstart.longpress', cancel);
  return () => {
    cancel();
    node.off('.longpress');
  };
}

/**
 * Two-finger pinch-zoom and pan on a stage. `getZoom/setZoom` adjust the
 * editor zoom; the stage position is panned so the pinch centre stays put.
 */
export function enablePinchZoom(
  stage: Konva.Stage,
  getZoom: () => number,
  setZoom: (zoom: number) => void,
  onGestureStart?: () => void,
): void {
  let lastDist = 0;
  let lastCenter: { x: number; y: number } | null = null;

  stage.on('touchmove.pinch', (e) => {
    const touches = e.evt.touches;
    if (touches.length !== 2) return;
    e.evt.preventDefault();
    const rect = stage.container().getBoundingClientRect();
    const p1 = { x: touches[0].clientX - rect.left, y: touches[0].clientY - rect.top };
    const p2 = { x: touches[1].clientX - rect.left, y: touches[1].clientY - rect.top };
    const center = { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
    const dist = Math.hypot(p2.x - p1.x, p2.y - p1.y);

    if (!lastCenter) {
      lastCenter = center;
      lastDist = dist;
      onGestureStart?.();
      return;
    }

    const oldZoom = getZoom();
    const pointTo = {
      x: (center.x - stage.x()) / oldZoom,
      y: (center.y - stage.y()) / oldZoom,
    };
    setZoom(Math.max(0.25, Math.min(3, oldZoom * (dist / lastDist))));
    const zoom = getZoom();
    stage.position({
      x: center.x - pointTo.x * zoom + (center.x - lastCenter.x),
      y: center.y - pointTo.y * zoom + (center.y - lastCenter.y),
    });
    stage.batchDraw();
    lastDist = dist;
    lastCenter = center;
  });

  stage.on('touchend.pinch touchcancel.pinch', () => {
    lastCenter = null;
    lastDist = 0;
  });
}

const MIN_ZOOM = 0.25;
const MAX_ZOOM = 3;

/** Zoom to `next`, keeping the stage point under `at` (container coords) fixed. */
function zoomAt(stage: Konva.Stage, getZoom: () => number, setZoom: (z: number) => void, next: number, at: { x: number; y: number }): void {
  const old = getZoom();
  const pointTo = { x: (at.x - stage.x()) / old, y: (at.y - stage.y()) / old };
  setZoom(Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, next)));
  const zoom = getZoom();
  stage.position({ x: at.x - pointTo.x * zoom, y: at.y - pointTo.y * zoom });
  stage.batchDraw();
}

/** A notched mouse wheel (as opposed to a trackpad's continuous scroll). */
function isMouseWheel(e: WheelEvent): boolean {
  if (e.deltaMode !== 0) return true;
  return e.deltaX === 0 && Math.abs(e.deltaY) >= 40 && Number.isInteger(e.deltaY);
}

/**
 * Wheel and trackpad navigation: a mouse wheel zooms in steps (as in the
 * original app); trackpad two-finger scrolling pans; pinching (WebKit
 * gesture events on macOS and iPadOS trackpads, or ⌘/Ctrl + wheel) zooms
 * smoothly around the pointer.
 */
export function enableWheelNavigation(
  stage: Konva.Stage,
  getZoom: () => number,
  setZoom: (zoom: number) => void,
): void {
  const container = stage.container();
  const local = (clientX: number, clientY: number) => {
    const r = container.getBoundingClientRect();
    return { x: clientX - r.left, y: clientY - r.top };
  };

  container.addEventListener('wheel', (e) => {
    e.preventDefault();
    const at = local(e.clientX, e.clientY);
    if (e.ctrlKey || e.metaKey) {
      zoomAt(stage, getZoom, setZoom, getZoom() * Math.exp(-e.deltaY * 0.01), at);
    } else if (isMouseWheel(e)) {
      const step = 1.1;
      zoomAt(stage, getZoom, setZoom, e.deltaY > 0 ? getZoom() / step : getZoom() * step, at);
    } else {
      stage.position({ x: stage.x() - e.deltaX, y: stage.y() - e.deltaY });
      stage.batchDraw();
    }
  }, { passive: false });

  // WebKit trackpad pinch: GestureEvent with a cumulative `scale`. iPadOS
  // also fires these for touch pinches, which enablePinchZoom handles, so
  // they are ignored while fingers are on the screen.
  type GestureEvent = UIEvent & { scale: number; clientX: number; clientY: number };
  let touches = 0;
  const trackTouches = (e: TouchEvent) => { touches = e.touches.length; };
  for (const type of ['touchstart', 'touchend', 'touchcancel'] as const) {
    container.addEventListener(type, trackTouches, { passive: true });
  }
  let startZoom = 1;
  container.addEventListener('gesturestart', (e) => {
    e.preventDefault();
    startZoom = getZoom();
  });
  container.addEventListener('gesturechange', (e) => {
    e.preventDefault();
    if (touches > 0) return;
    const g = e as GestureEvent;
    zoomAt(stage, getZoom, setZoom, startZoom * g.scale, local(g.clientX, g.clientY));
  });
  container.addEventListener('gestureend', (e) => e.preventDefault());
}
