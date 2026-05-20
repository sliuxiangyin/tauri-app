import * as React from "react";
import { Textarea } from "@/components/ui/textarea";

interface JsonEditorProps {
  value: Record<string, any>;
  onChange: (value: Record<string, any>) => void;
}

export function JsonEditor({ value, onChange }: JsonEditorProps) {
  // 直接使用 value 字符串化，不做任何转换，保留用户原始输入
  const code = JSON.stringify(value, null, 2);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const text = e.target.value;
    try {
      const parsed = JSON.parse(text);
      onChange(parsed);
    } catch (e) {
      // JSON 解析失败时不更新，保持当前有效值
    }
  };

  return (
        <Textarea
          value={code}
          onChange={handleChange}
          className="min-h-[300px] font-mono text-sm"
          spellCheck={false}
        />
  );
}