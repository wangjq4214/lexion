import * as stylex from "@stylexjs/stylex";
import reactLogo from "../assets/react.svg";
import { tokens } from "../styles/tokens.stylex";

const styles = stylex.create({
  row: {
    display: "flex",
    justifyContent: "center",
  },
  link: {
    textDecoration: "inherit",
    color: {
      default: tokens.link,
      ":hover": tokens.linkHover,
    },
    fontWeight: 500,
  },
  logo: {
    padding: "1.5em",
    transition: "0.75s",
    willChange: "filter",
    height: "6em",
  },
  viteLogo: {
    filter: { ":hover": "drop-shadow(0 0 2em #747bff)" },
  },
  tauriLogo: {
    filter: { ":hover": "drop-shadow(0 0 2em #24c8db)" },
  },
  reactLogo: {
    filter: { ":hover": "drop-shadow(0 0 2em #61dafb)" },
  },
});

export function BrandLinks() {
  return (
    <div {...stylex.props(styles.row)}>
      <a
        {...stylex.props(styles.link)}
        href="https://vite.dev"
        target="_blank"
        rel="noopener"
      >
        <img
          {...stylex.props(styles.logo, styles.viteLogo)}
          src="/vite.svg"
          alt="Vite logo"
        />
      </a>
      <a
        {...stylex.props(styles.link)}
        href="https://tauri.app"
        target="_blank"
        rel="noopener"
      >
        <img
          {...stylex.props(styles.logo, styles.tauriLogo)}
          src="/tauri.svg"
          alt="Tauri logo"
        />
      </a>
      <a
        {...stylex.props(styles.link)}
        href="https://react.dev"
        target="_blank"
        rel="noopener"
      >
        <img
          {...stylex.props(styles.logo, styles.reactLogo)}
          src={reactLogo}
          alt="React logo"
        />
      </a>
    </div>
  );
}
