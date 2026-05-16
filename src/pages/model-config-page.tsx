import * as React from "react";

import { ScrollArea } from "@/components/ui/scroll-area";
import {
  ConfigEditorForm,
  ProfileList,
  createDefaultProfile,
  type ModelConfigProfile,
} from "@/components/model-config-page";
import {
  createConfig,
  deleteConfigById,
  fetchAllConfigs,
  updateConfig,
} from "@/lib/model-config-api";
import { SiderContent } from "@/components/common/sider-content";
import { ProfileListItem } from "@/components/model-config-page/config-profile-list-item";

export default function ModelConfigPage() {
  const [profiles, setProfiles] = React.useState<ModelConfigProfile[]>([]);
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  // IDs of profiles that were created locally and haven't been persisted yet.
  const newIdsRef = React.useRef<Set<string>>(new Set());
  // Snapshot of DB profiles keyed by id, used for model diff on update.
  const dbSnapshotRef = React.useRef<Map<string, ModelConfigProfile>>(new Map());

  const selected = profiles.find((p) => p.id === selectedId) ?? null;
  const isNew = selected ? newIdsRef.current.has(selected.id) : false;

  // ---- Init: load from backend ----
  React.useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const list = await fetchAllConfigs();
        if (cancelled) return;
        setProfiles(list);
        const snap = new Map<string, ModelConfigProfile>();
        for (const p of list) snap.set(p.id, p);
        dbSnapshotRef.current = snap;
        newIdsRef.current = new Set();
        // Auto-select first profile if any
        if (list.length > 0) {
          setSelectedId(list[0].id);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  // ---- Handlers ----
  const addProfile = () => {
    const p = createDefaultProfile();
    setProfiles((prev) => [...prev, p]);
    console.log("Added new profil2e:", p);

    newIdsRef.current = new Set(newIdsRef.current).add(p.id);
    setSelectedId(p.id);
    setError(null);
    console.log("Added new profile:", p);
  };

  const updateProfile = (next: ModelConfigProfile) => {
    setProfiles((prev) => prev.map((p) => (p.id === next.id ? next : p)));
  };

  const handleSave = async () => {
    if (!selected || saving) return;
    setError(null);
    setSaving(true);
    try {
      if (isNew) {
        await createConfig(selected);
        newIdsRef.current = new Set(
          [...newIdsRef.current].filter((id) => id !== selected.id),
        );
      } else {
        const prevModelIds = new Set(
          (dbSnapshotRef.current.get(selected.id)?.models ?? []).map((m) => m.id),
        );
        await updateConfig(selected, prevModelIds);
      }
      // Update snapshot
      const snap = new Map(dbSnapshotRef.current);
      snap.set(selected.id, selected);
      dbSnapshotRef.current = snap;
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!selected) return;
    const id = selected.id;
    setError(null);

    // Optimistic UI
    setProfiles((prev) => prev.filter((p) => p.id !== id));
    setSelectedId((cur) => (cur === id ? null : cur));

    if (newIdsRef.current.has(id)) {
      // Not yet persisted — just remove from tracking
      newIdsRef.current = new Set(
        [...newIdsRef.current].filter((x) => x !== id),
      );
      return;
    }

    try {
      await deleteConfigById(id);
      const snap = new Map(dbSnapshotRef.current);
      snap.delete(id);
      dbSnapshotRef.current = snap;
    } catch (e) {
      setError(String(e));
      // Refetch to reconcile
      try {
        const list = await fetchAllConfigs();
        setProfiles(list);
        const snap = new Map<string, ModelConfigProfile>();
        for (const p of list) snap.set(p.id, p);
        dbSnapshotRef.current = snap;
        newIdsRef.current = new Set();
      } catch {
        // ignore double-failure
      }
    }
  };

  // ---- Render ----
  if (loading) {
    return (
      <div className="flex h-full w-full flex-col  bg-white">
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">加载中…</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full w-full flex-col bg-white">
      {error ? (
        <div className="mx-4 mt-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
          {error}
          <button
            type="button"
            className="ml-2 underline"
            onClick={() => setError(null)}
          >
            关闭
          </button>
        </div>
      ) : null}
      <SiderContent
        className="min-h-0 flex-1"
        list={
          <ProfileList
            profiles={profiles}
            onAdd={addProfile}
            itmeNode={profiles.map((p) => (
              <ProfileListItem
                key={p.id}
                profile={p}
                selected={p.id === selectedId}
                onSelect={(id) => setSelectedId(id)}
                onDelete={(id) => {
                  if (id === selectedId) {
                    handleDelete();
                  } else {
                    // Deleting a non-selected profile via list context menu
                    setProfiles((prev) => prev.filter((p) => p.id !== id));
                    deleteConfigById(id).catch((e) => setError(String(e)));
                  }
                }}
              />
            ))}

          />
        }
        editor={
          selected ? (
            <ScrollArea className="h-full min-h-0">
              <div className="p-4">
                <ConfigEditorForm
                  profile={selected}
                  isNew={isNew}
                  saving={saving}
                  onChange={updateProfile}
                  onSave={handleSave}
                  onDelete={handleDelete}
                />
              </div>
            </ScrollArea>
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-12 text-center">
              <p className="text-sm text-muted-foreground">
                请从左侧选择一条配置，或点击「添加」新建。
              </p>
            </div>
          )
        }
      />
    </div>
  );
}
