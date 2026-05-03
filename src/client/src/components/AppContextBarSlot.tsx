import React from "react";
import { ContextBar } from "./ContextBar";

type AppContextBarSlotProps = React.ComponentProps<typeof ContextBar>;

export function AppContextBarSlot(props: AppContextBarSlotProps): React.JSX.Element {
  return <ContextBar {...props} />;
}
