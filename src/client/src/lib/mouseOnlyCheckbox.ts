import type React from "react";

function blurInputSoon(input: HTMLInputElement): void {
  window.requestAnimationFrame(() => {
    input.blur();
  });
}

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

export const mouseOnlyRangeInputProps = {
  tabIndex: -1,
  onFocus: (event: React.FocusEvent<HTMLInputElement>) => {
    blurInputSoon(event.currentTarget);
  },
  onKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.code === "Space" || event.key === " " || event.key === "Enter") {
      event.preventDefault();
    }
  },
  onPointerUp: (event: React.PointerEvent<HTMLInputElement>) => {
    blurInputSoon(event.currentTarget);
  },
  onPointerCancel: (event: React.PointerEvent<HTMLInputElement>) => {
    blurInputSoon(event.currentTarget);
  }
} satisfies Pick<React.InputHTMLAttributes<HTMLInputElement>, "tabIndex" | "onFocus" | "onKeyDown" | "onPointerUp" | "onPointerCancel">;
