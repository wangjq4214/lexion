import * as stylex from "@stylexjs/stylex";
import { invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useState } from "react";
import { BrandLinks } from "./components/BrandLinks";
import { GreetingForm } from "./components/GreetingForm";
import { tokens } from "./styles/tokens.stylex";

const styles = stylex.create({
  documentRoot: {
    MozOsxFontSmoothing: "grayscale",
    WebkitFontSmoothing: "antialiased",
    fontSynthesis: "none",
    backgroundColor: tokens.appBackground,
    color: tokens.appText,
    fontFamily: "Inter, Avenir, Helvetica, Arial, sans-serif",
    fontSize: 16,
    fontWeight: 400,
    lineHeight: "24px",
    textRendering: "optimizeLegibility",
    textSizeAdjust: "100%",
  },
  appShell: {
    margin: 0,
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
    textAlign: "center",
    paddingTop: "10vh",
  },
});

function App() {
  const [greetMsg, setGreetMsg] = useState("");

  useLayoutEffect(() => {
    const { className } = stylex.props(styles.documentRoot);
    const classNames = className?.split(" ") ?? [];
    document.documentElement.classList.add(...classNames);

    return () => {
      document.documentElement.classList.remove(...classNames);
    };
  }, []);

  async function greet(name: string) {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name }));
  }

  return (
    <main {...stylex.props(styles.appShell)}>
      <h1>Welcome to Tauri + React</h1>
      <BrandLinks />
      <p>Click on the Tauri, Vite, and React logos to learn more.</p>
      <GreetingForm onGreet={greet} />
      <p>{greetMsg}</p>
    </main>
  );
}

export default App;
