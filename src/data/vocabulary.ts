export type WordEntry = {
  id: string;
  english: string;
  chinese: string;
};

export const WORDS: readonly WordEntry[] = [
  { id: "apple", english: "apple", chinese: "苹果" },
  { id: "book", english: "book", chinese: "书" },
  { id: "cat", english: "cat", chinese: "猫" },
  { id: "dog", english: "dog", chinese: "狗" },
  { id: "water", english: "water", chinese: "水" },
  { id: "house", english: "house", chinese: "房子" },
  { id: "friend", english: "friend", chinese: "朋友" },
  { id: "school", english: "school", chinese: "学校" },
  { id: "happy", english: "happy", chinese: "快乐" },
  { id: "beautiful", english: "beautiful", chinese: "美丽" },
];
