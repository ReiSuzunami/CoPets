export function normalizeControlAnswers(values = {}) {
  return Object.fromEntries(
    Object.entries(values)
      .filter(([, value]) => typeof value === "string" && value.trim().length > 0)
      .map(([id, value]) => [id, [value.trim()]]),
  );
}

export function prepareFollowUp(value) {
  if (typeof value !== "string") return null;
  return value.trim() || null;
}

export function visibleAnswer(question, value) {
  return question?.options?.find((option) => option.id === value)?.label || value || "";
}
