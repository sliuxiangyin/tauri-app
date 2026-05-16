import Editor from "react-simple-code-editor";
import { highlight, languages } from "prismjs";
import "prismjs/components/prism-json";
import "prismjs/themes/prism-tomorrow.css";
import { Card, CardContent } from "@/components/ui/card";

interface JsonEditorProps {
  value: Record<string, any>;
  onChange: (value: Record<string, any>) => void;
}

function removeEmptyValues(obj: Record<string, any>): Record<string, any> {
  const result: Record<string, any> = {};
  for (const [key, val] of Object.entries(obj)) {
    if (val === null || val === undefined) continue;
    if (typeof val === "object" && !Array.isArray(val)) {
      const nested = removeEmptyValues(val);
      if (Object.keys(nested).length > 0) {
        result[key] = nested;
      }
    } else if (Array.isArray(val)) {
      if (val.length > 0) {
        result[key] = val;
      }
    } else if (val !== "" && val !== false) {
      result[key] = val;
    }
  }
  return result;
}

export function JsonEditor({ value, onChange }: JsonEditorProps) {
  const code = JSON.stringify(removeEmptyValues(value), null, 2);

  const handleChange = (text: string) => {
    try {
      const parsed = JSON.parse(text);
      onChange(parsed);
    } catch (e) {}
  };

  return (
    <Card>
      <CardContent className="p-0">
        <Editor
          value={code}
          onValueChange={handleChange}
          highlight={(code) => highlight(code, languages.json, "json")}
          padding={16}
          style={{
            fontFamily: '"Fira Code", monospace',
            fontSize: 12,
            minHeight: "300px",
          }}
        />
      </CardContent>
    </Card>
  );
}