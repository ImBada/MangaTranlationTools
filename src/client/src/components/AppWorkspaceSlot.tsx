import React from "react";
import { WorkspacePanel } from "./workspace/WorkspacePanel";

type AppWorkspaceSlotProps = React.ComponentProps<typeof WorkspacePanel>;

export function AppWorkspaceSlot(props: AppWorkspaceSlotProps): React.JSX.Element {
  return <WorkspacePanel {...props} />;
}
