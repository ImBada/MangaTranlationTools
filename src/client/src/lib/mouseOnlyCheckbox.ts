import type React from "react";

export const mouseOnlyCheckboxProps = {
  tabIndex: -1,
  onFocus: (event: React.FocusEvent<HTMLInputElement>) => {
    event.currentTarget.blur();
  },
  onKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.code === "Space" || event.key === " ") {
      event.preventDefault();
    }
  }
} satisfies Pick<React.InputHTMLAttributes<HTMLInputElement>, "tabIndex" | "onFocus" | "onKeyDown">;
