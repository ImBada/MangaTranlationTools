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

export const mouseOnlyColorInputProps = {
  tabIndex: -1,
  onKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.code === "Space" || event.key === " " || event.key === "Enter") {
      event.preventDefault();
    }
  },
  onClick: (event: React.MouseEvent<HTMLInputElement>) => {
    blurInputSoon(event.currentTarget);
  },
  onInput: (event: React.FormEvent<HTMLInputElement>) => {
    blurInputSoon(event.currentTarget);
  }
} satisfies Pick<React.InputHTMLAttributes<HTMLInputElement>, "tabIndex" | "onKeyDown" | "onClick" | "onInput">;
