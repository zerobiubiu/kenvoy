import type { ApplicationDescription, ComponentDefinition, NavigationItem } from '../types.ts';
import type { Translate } from './model.ts';

function freeze<T>(value: T): T {
  if (value !== null && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

/** 翻译独立显示视图；标识、路径、条件、默认值和业务记录从不参与词条替换。 */
export function localizeApplication(source: ApplicationDescription, t: Translate): ApplicationDescription {
  const text = (value: string) => t(`text:${value}`, undefined, value);
  const node = (component: ComponentDefinition): ComponentDefinition => {
    switch (component.type) {
      case 'text': return { ...component, value: text(component.value) };
      case 'image': return { ...component, alt: text(component.alt) };
      case 'form': return { ...component, labels: Object.fromEntries(Object.entries(component.labels).map(([name, label]) => [name, text(label)])) };
      case 'group': return { ...component, label: component.label === null ? null : text(component.label), children: component.children.map(node) };
      case 'custom': return { ...component, children: component.children.map(node) };
      case 'table': return component;
    }
  };
  const navigation = (item: NavigationItem): NavigationItem => item.type === 'link'
    ? { ...item, label: text(item.label) }
    : { ...item, label: item.label === null ? null : text(item.label), items: item.items.map(navigation) };
  return freeze({
    ...source,
    name: t('app.name', undefined, source.name),
    resources: source.resources.map(resource => ({
      ...resource,
      label: t(`resources.${resource.name}.label`, undefined, resource.label ?? resource.name),
      fields: resource.fields.map(field => ({
        ...field,
        label: t(`resources.${resource.name}.fields.${field.name}.label`, undefined, field.label),
        options: field.options.map(option => ({ ...option,
          label: t(`resources.${resource.name}.fields.${field.name}.options.${option.value}.label`, undefined, option.label),
        })),
      })),
    })),
    pages: source.pages.map(page => ({ ...page, title: t(`pages.${page.path}.title`, undefined, page.title), components: page.components.map(node) })),
    navigation: source.navigation.map(group => ({ ...group, label: group.label === null ? null : text(group.label), items: group.items.map(navigation) })),
  });
}
