export function shouldShowFollowUp(control = {}) {
  return Boolean(
    control.canReply
    || control.canStartFollowUp
    || control.showWorkingFollowUp
    || control.showReadyFollowUp,
  );
}
