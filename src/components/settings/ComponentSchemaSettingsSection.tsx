import { useEffect, useMemo, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Boxes, CheckCircle2, FolderOpen, Loader2, Save } from 'lucide-react';
import {
  getComponentSettings,
  saveComponentSettings,
  type ComponentSettingsSnapshot,
} from '../../api/componentSettings';
import { useWorkspaceProfileStore } from '../../stores/workspaceProfileStore';
import type {
  ComponentSettingsSection,
  JsonValue,
  SettingsField,
  SettingsScope,
} from '../../types/platform';

interface ComponentSchemaSettingsSectionProps {
  scope: SettingsScope;
  projectPath?: string | null;
}

interface ComponentSectionTarget {
  componentId: string;
  componentName: string;
  componentVersion: string;
  section: ComponentSettingsSection;
}

export function ComponentSchemaSettingsSection({
  scope,
  projectPath,
}: ComponentSchemaSettingsSectionProps) {
  const components = useWorkspaceProfileStore((state) => state.snapshot?.components ?? []);
  const targets = useMemo<ComponentSectionTarget[]>(() => components
    .filter((component) => component.effectiveEnabled)
    .flatMap((component) => component.settingsSections
      .filter((section) => section.scope === scope)
      .map((section) => ({
        componentId: component.id,
        componentName: component.name,
        componentVersion: component.version,
        section,
      })))
    .sort((left, right) => (
      (left.section.order ?? 0) - (right.section.order ?? 0)
      || left.componentName.localeCompare(right.componentName)
      || left.section.title.localeCompare(right.section.title)
    )), [components, scope]);

  return (
    <div className="space-y-4">
      <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
        <div className="flex items-start gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-violet-100 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300">
            <Boxes className="h-4 w-4" />
          </div>
          <div>
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
              {scope === 'global' ? '全局组件设置' : '项目组件设置'}
            </h4>
            <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-gray-400">
              由已启用组件的版本化 Schema 生成。组件停用后表单撤下，但保存的数据会保留。
            </p>
          </div>
        </div>
      </section>

      {targets.map((target) => (
        <ComponentSettingsCard
          key={`${target.componentId}:${target.section.id}`}
          target={target}
          scope={scope}
          projectPath={projectPath}
        />
      ))}

      {targets.length === 0 ? (
        <div className="rounded-xl border border-dashed border-gray-300 bg-white px-4 py-8 text-center text-sm text-gray-500 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-400">
          当前生效组件没有声明{scope === 'global' ? '全局' : '项目'} Schema 设置。
        </div>
      ) : null}
    </div>
  );
}

function ComponentSettingsCard({
  target,
  scope,
  projectPath,
}: {
  target: ComponentSectionTarget;
  scope: SettingsScope;
  projectPath?: string | null;
}) {
  const [snapshot, setSnapshot] = useState<ComponentSettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<Record<string, JsonValue>>({});
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setIsLoading(true);
    setError(null);
    void getComponentSettings({
      componentId: target.componentId,
      sectionId: target.section.id,
      scope,
      projectPath: projectPath ?? null,
    }).then((result) => {
      if (cancelled) return;
      setSnapshot(result);
      setDraft(result.values);
    }).catch((nextError) => {
      if (!cancelled) setError(String(nextError));
    }).finally(() => {
      if (!cancelled) setIsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [projectPath, scope, target.componentId, target.section.id]);

  const updateValue = (fieldId: string, value: JsonValue) => {
    setDraft((current) => ({ ...current, [fieldId]: value }));
    setMessage(null);
  };

  const save = async () => {
    setIsSaving(true);
    setError(null);
    setMessage(null);
    try {
      const result = await saveComponentSettings({
        componentId: target.componentId,
        sectionId: target.section.id,
        scope,
        projectPath: projectPath ?? null,
        values: draft,
      });
      setSnapshot(result);
      setDraft(result.values);
      setMessage('已保存');
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{target.section.title}</h4>
            <span className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-[11px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">
              {target.componentName} · v{target.componentVersion}
            </span>
          </div>
          {target.section.description ? (
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{target.section.description}</p>
          ) : null}
        </div>
        <button
          type="button"
          onClick={() => void save()}
          disabled={isLoading || isSaving || (scope === 'project' && !projectPath)}
          className="inline-flex h-8 items-center gap-1.5 rounded-md bg-blue-600 px-3 text-xs font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
          保存
        </button>
      </div>

      {isLoading ? (
        <div className="mt-4 flex items-center gap-2 text-xs text-gray-500">
          <Loader2 className="h-4 w-4 animate-spin" />读取组件设置...
        </div>
      ) : (
        <div className="mt-4 space-y-4">
          {target.section.fields.map((field) => (
            <SettingsFieldControl
              key={field.id}
              field={field}
              value={draft[field.id]}
              onChange={(value) => updateValue(field.id, value)}
            />
          ))}
        </div>
      )}

      {message ? (
        <p className="mt-3 flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-300">
          <CheckCircle2 className="h-3.5 w-3.5" />{message}
        </p>
      ) : null}
      {error ? <p className="mt-3 break-all text-xs text-red-600 dark:text-red-300">{error}</p> : null}
      {snapshot?.storagePath ? (
        <p className="mt-3 break-all font-mono text-[10px] text-gray-400">{snapshot.storagePath}</p>
      ) : null}
    </section>
  );
}

function SettingsFieldControl({
  field,
  value,
  onChange,
}: {
  field: SettingsField;
  value: JsonValue | undefined;
  onChange: (value: JsonValue) => void;
}) {
  if (field.type === 'boolean') {
    return (
      <label className="flex items-start gap-3 text-sm text-gray-700 dark:text-gray-300">
        <input
          type="checkbox"
          checked={value === true}
          onChange={(event) => onChange(event.target.checked)}
          className="mt-0.5 rounded"
        />
        <span>{field.label}{field.description ? <FieldDescription text={field.description} /> : null}</span>
      </label>
    );
  }

  const label = (
    <div className="mb-1.5">
      <span className="text-sm font-medium text-gray-800 dark:text-gray-200">{field.label}</span>
      {field.required ? <span className="ml-1 text-red-500">*</span> : null}
      {field.description ? <FieldDescription text={field.description} /> : null}
    </div>
  );

  if (field.type === 'enum') {
    return (
      <label className="block">
        {label}
        <select
          value={typeof value === 'string' ? value : ''}
          onChange={(event) => onChange(event.target.value)}
          className="h-9 w-full rounded-md border border-gray-200 bg-white px-3 text-sm text-gray-800 outline-none focus:border-blue-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-200"
        >
          {(field.options ?? []).map((option) => (
            <option key={option.value} value={option.value}>{option.label}</option>
          ))}
        </select>
      </label>
    );
  }

  if (field.type === 'string-list') {
    const lines = Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
    return (
      <label className="block">
        {label}
        <textarea
          value={lines.join('\n')}
          onChange={(event) => onChange(event.target.value.split(/\r?\n/).filter(Boolean))}
          placeholder={field.placeholder}
          rows={4}
          className="w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-800 outline-none focus:border-blue-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-200"
        />
      </label>
    );
  }

  const isNumber = field.type === 'integer' || field.type === 'number';
  const isPath = field.type === 'path' || field.type === 'file' || field.type === 'directory';
  const textValue = typeof value === 'string' || typeof value === 'number' ? String(value) : '';
  const input = (
    <input
      type={field.sensitive ? 'password' : isNumber ? 'number' : 'text'}
      value={textValue}
      min={field.minimum}
      max={field.maximum}
      step={field.type === 'integer' ? 1 : field.type === 'number' ? 'any' : undefined}
      placeholder={field.placeholder}
      onChange={(event) => {
        if (isNumber) {
          const parsed = field.type === 'integer'
            ? Number.parseInt(event.target.value, 10)
            : Number.parseFloat(event.target.value);
          onChange(Number.isFinite(parsed) ? parsed : 0);
          return;
        }
        onChange(event.target.value);
      }}
      className="h-9 min-w-0 flex-1 rounded-md border border-gray-200 bg-white px-3 text-sm text-gray-800 outline-none focus:border-blue-400 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-200"
    />
  );

  return (
    <label className="block">
      {label}
      {isPath ? (
        <div className="flex gap-2">
          {input}
          <button
            type="button"
            onClick={() => void selectPath(field).then((selected) => {
              if (selected) onChange(selected);
            })}
            className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-gray-200 text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
            title="选择路径"
          >
            <FolderOpen className="h-4 w-4" />
          </button>
        </div>
      ) : input}
    </label>
  );
}

function FieldDescription({ text }: { text: string }) {
  return <span className="mt-1 block text-xs font-normal text-gray-500 dark:text-gray-400">{text}</span>;
}

async function selectPath(field: SettingsField) {
  const selected = await open({
    multiple: false,
    directory: field.type === 'directory' || field.type === 'path',
  });
  return typeof selected === 'string' ? selected : null;
}
