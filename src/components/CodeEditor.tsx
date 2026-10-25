import { python } from "@codemirror/lang-python";
import { oneDark } from "@codemirror/theme-one-dark";
import CodeMirror from "@uiw/react-codemirror";

export function CodeEditor({
  value,
  onChange,
  height = "420px",
  readOnly = false,
}: {
  value: string;
  onChange?: (v: string) => void;
  height?: string;
  readOnly?: boolean;
}) {
  return (
    <CodeMirror
      value={value}
      height={height}
      theme={oneDark}
      extensions={[python()]}
      editable={!readOnly}
      basicSetup={{ lineNumbers: true, highlightActiveLine: true, tabSize: 4 }}
      onChange={readOnly ? undefined : onChange}
    />
  );
}
