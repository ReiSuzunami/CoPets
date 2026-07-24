const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

export function createMotionPreference({ matchMedia, onChange }) {
  const media = matchMedia(REDUCED_MOTION_QUERY);
  let value = Boolean(media.matches);
  let destroyed = false;

  const handleChange = (event) => {
    if (destroyed) return;
    value = Boolean(event.matches);
    onChange(value);
  };

  media.addEventListener("change", handleChange);
  onChange(value);

  return {
    current: () => value,
    destroy() {
      if (destroyed) return;
      destroyed = true;
      media.removeEventListener("change", handleChange);
    },
  };
}
