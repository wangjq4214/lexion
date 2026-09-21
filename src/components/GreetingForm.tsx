import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { tokens } from "../styles/tokens.stylex";

type GreetingFormProps = {
  onGreet: (name: string) => void;
};

const styles = stylex.create({
  form: {
    display: "flex",
    justifyContent: "center",
  },
  control: {
    borderColor: tokens.controlBorder,
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    outline: "none",
    paddingBlock: "0.6em",
    paddingInline: "1.2em",
    transition: "border-color 0.25s",
    backgroundColor: tokens.controlBackground,
    boxShadow: "0 2px 2px rgba(0, 0, 0, 0.2)",
    color: tokens.controlText,
    fontFamily: "inherit",
    fontSize: "1em",
    fontWeight: 500,
  },
  input: {
    marginRight: 5,
  },
  button: {
    borderColor: {
      ":hover": tokens.buttonBorder,
      ":active": tokens.buttonBorder,
    },
    backgroundColor: { ":active": tokens.buttonActiveBackground },
    cursor: "pointer",
  },
});

export function GreetingForm({ onGreet }: GreetingFormProps) {
  const [name, setName] = useState("");

  return (
    <form
      {...stylex.props(styles.form)}
      onSubmit={(event) => {
        event.preventDefault();
        onGreet(name);
      }}
    >
      <input
        {...stylex.props(styles.control, styles.input)}
        id="greet-input"
        onChange={(event) => setName(event.currentTarget.value)}
        placeholder="Enter a name..."
      />
      <button {...stylex.props(styles.control, styles.button)} type="submit">
        Greet
      </button>
    </form>
  );
}
