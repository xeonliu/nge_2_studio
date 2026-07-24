import { useEffect, useRef } from "react";

export function useSplitter(onDelta: (delta: number) => void, direction: 1 | -1 = 1) {
  const dragging = useRef<{ x: number } | null>(null);

  useEffect(() => {
    const move = (event: PointerEvent) => {
      if (!dragging.current) return;
      const delta = (event.clientX - dragging.current.x) * direction;
      dragging.current.x = event.clientX;
      onDelta(delta);
    };
    const stop = () => {
      dragging.current = null;
      document.body.classList.remove("is-resizing");
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
  }, [direction, onDelta]);

  return {
    onPointerDown(event: React.PointerEvent) {
      dragging.current = { x: event.clientX };
      document.body.classList.add("is-resizing");
    },
  };
}

