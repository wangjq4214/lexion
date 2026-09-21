import * as stylex from "@stylexjs/stylex";

export const tokens = stylex.defineVars({
  appBackground: {
    default: "#f6f6f6",
    "@media (prefers-color-scheme: dark)": "#2f2f2f",
  },
  appText: {
    default: "#0f0f0f",
    "@media (prefers-color-scheme: dark)": "#f6f6f6",
  },
  buttonActiveBackground: {
    default: "#e8e8e8",
    "@media (prefers-color-scheme: dark)": "#0f0f0f69",
  },
  buttonBorder: "#396cd8",
  controlBackground: {
    default: "#ffffff",
    "@media (prefers-color-scheme: dark)": "#0f0f0f98",
  },
  controlBorder: "transparent",
  controlText: {
    default: "#0f0f0f",
    "@media (prefers-color-scheme: dark)": "#ffffff",
  },
  link: "#646cff",
  linkHover: {
    default: "#535bf2",
    "@media (prefers-color-scheme: dark)": "#24c8db",
  },
});
