"use client"

import { CheckIcon, CircleIcon, Loader2Icon, XIcon } from "lucide-react"

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { cn } from "@/lib/utils"
import type { PlanStepDto } from "@/lib/api/tauri-llm"

interface PlanStepsListProps {
  reasoning?: string
  steps: PlanStepDto[]
  className?: string
}

/** 步骤状态 */
type StepStatus = "pending" | "in_progress" | "completed" | "failed"

function getStepStatus(_step: PlanStepDto): StepStatus {
  // TODO: 后续由后端推送状态更新，暂返回 pending
  return "pending"
}

function StatusIcon({ status, className }: { status: StepStatus; className?: string }) {
  switch (status) {
    case "pending":
      return <CircleIcon className={cn("size-4 text-muted-foreground shrink-0", className)} />
    case "in_progress":
      return <Loader2Icon className={cn("size-4 text-blue-500 shrink-0 animate-spin", className)} />
    case "completed":
      return <CheckIcon className={cn("size-4 text-green-500 shrink-0", className)} />
    case "failed":
      return <XIcon className={cn("size-4 text-red-500 shrink-0", className)} />
  }
}

export function StepTypeBadge({ stepType }: { stepType: string }) {
  // step_type: "tool_call" / "exploratory" / "final_answer"
  const map: Record<string, { label: string; variant: "default" | "secondary" | "outline" }> = {
    tool_call: { label: "工具", variant: "secondary" },
    exploratory: { label: "探索", variant: "outline" },
    final_answer: { label: "回答", variant: "default" },
  }
  const config = map[stepType] ?? { label: stepType, variant: "outline" as const }

  return (
    <span
      className={cn(
        "inline-flex h-5 shrink-0 items-center rounded-4xl border px-1.5 py-0.5 text-[10px] font-medium",
        config.variant === "default" && "border-transparent bg-primary text-primary-foreground",
        config.variant === "secondary" && "border-border bg-secondary text-secondary-foreground",
        config.variant === "outline" && "border-border text-foreground"
      )}
    >
      {config.label}
    </span>
  )
}

export function PlanStepsList({ reasoning, steps, className }: PlanStepsListProps) {
  if (!steps || steps.length === 0) return null

  return (
    <div className="w-full">
    <Card size="sm" className={cn("mt-2 w-full", className)}>
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="text-xs font-medium text-muted-foreground">
            📋 执行计划
          </CardTitle>
          <span className="text-[10px] text-muted-foreground">
            {steps.length} 个步骤
          </span>
        </div>
        {reasoning && (
          <p className="text-xs text-muted-foreground/70 ">
            {reasoning}
          </p>
        )}
      </CardHeader>

      <CardContent className="grid gap-2 pb-3">
        {steps.map((step) => {
          const status = getStepStatus(step)
          return (
            <div
              key={step.order}
              className="flex items-start gap-2 rounded-lg border border-transparent px-3 py-2 transition-colors hover:border-border/50"
            >
              {/* 状态图标 */}
              <div className="mt-0.5">
                <StatusIcon status={status} />
              </div>

              {/* 步骤信息 */}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="text-xs font-medium text-foreground">
                    {step.order}.
                  </span>
                {step.step_goal && (
                  <p className="mt-1 text-xs font-medium text-foreground/90 line-clamp-2">
                    {step.step_goal}
                  </p>
                )}
                </div>
               
                {step.tool_name && (
                  <code className="mt-1 inline-block rounded bg-muted px-1.5 py-0.5 text-[11px] font-medium text-foreground/80">
                    {step.tool_name}
                  </code>
                )}
              </div>
            </div>
          )
        })}
      </CardContent>
    </Card>
    </div>
  )
}