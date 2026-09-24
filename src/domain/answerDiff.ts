export type AnswerSegment = { text: string; changed: boolean };

// Bound the quadratic alignment work; very long mismatches still retain their
// matching prefix and suffix, with the unmatched middle marked as a whole.
const MAX_ALIGNMENT_CELLS = 100_000;

export function diffAnswers(
  submitted: string,
  expected: string,
): { submitted: AnswerSegment[]; expected: AnswerSegment[] } {
  const left = Array.from(submitted);
  const right = Array.from(expected);
  const submittedParts: AnswerSegment[] = [];
  const expectedParts: AnswerSegment[] = [];
  const append = (parts: AnswerSegment[], text: string, changed: boolean) => {
    const last = parts[parts.length - 1];
    if (last?.changed === changed) last.text += text;
    else if (text) parts.push({ text, changed });
  };

  let prefix = 0;
  while (
    prefix < left.length &&
    prefix < right.length &&
    left[prefix] === right[prefix]
  ) {
    prefix += 1;
  }
  let suffix = 0;
  while (
    suffix < left.length - prefix &&
    suffix < right.length - prefix &&
    left[left.length - suffix - 1] === right[right.length - suffix - 1]
  ) {
    suffix += 1;
  }
  const start = left.slice(0, prefix).join("");
  append(submittedParts, start, false);
  append(expectedParts, start, false);
  const middleLeft = left.slice(prefix, left.length - suffix);
  const middleRight = right.slice(prefix, right.length - suffix);

  if (middleLeft.length * middleRight.length > MAX_ALIGNMENT_CELLS) {
    append(submittedParts, middleLeft.join(""), true);
    append(expectedParts, middleRight.join(""), true);
  } else {
    const lengths = Array.from(
      { length: middleLeft.length + 1 },
      () => new Uint32Array(middleRight.length + 1),
    );
    for (let i = middleLeft.length - 1; i >= 0; i -= 1) {
      for (let j = middleRight.length - 1; j >= 0; j -= 1) {
        lengths[i][j] =
          middleLeft[i] === middleRight[j]
            ? 1 + lengths[i + 1][j + 1]
            : Math.max(lengths[i + 1][j], lengths[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < middleLeft.length || j < middleRight.length) {
      if (
        i < middleLeft.length &&
        j < middleRight.length &&
        middleLeft[i] === middleRight[j]
      ) {
        append(submittedParts, middleLeft[i], false);
        append(expectedParts, middleRight[j], false);
        i += 1;
        j += 1;
      } else if (
        i < middleLeft.length &&
        (j === middleRight.length || lengths[i + 1][j] >= lengths[i][j + 1])
      ) {
        append(submittedParts, middleLeft[i], true);
        i += 1;
      } else {
        append(expectedParts, middleRight[j], true);
        j += 1;
      }
    }
  }
  const end = suffix ? left.slice(left.length - suffix).join("") : "";
  append(submittedParts, end, false);
  append(expectedParts, end, false);
  return { submitted: submittedParts, expected: expectedParts };
}
