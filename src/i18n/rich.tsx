import { Fragment, useCallback, useContext, type ReactNode } from 'react';

import { LocaleContext, translate, type TranslateVars } from './core';

export type RichTFunction = (
  key: string,
  vars?: TranslateVars,
  mono?: readonly string[],
  titles?: Readonly<Record<string, string>>,
) => ReactNode;

/** `tr(key, vars, mono?, titles?)`: like `t()`, but returns nodes and wraps
 * the chosen values (all by default) in mono tabular spans (brand §7.6).
 * `titles` gives a value its own tooltip (a shortened path's full text). */
export function useRichT(): RichTFunction {
  const locale = useContext(LocaleContext);
  return useCallback(
    (
      key: string,
      vars: TranslateVars = {},
      mono?: readonly string[],
      titles?: Readonly<Record<string, string>>,
    ) => {
      const template = translate(locale, key);
      return template.split(/(\{\w+\})/g).map((part, i) => {
        const name = /^\{(\w+)\}$/.exec(part)?.[1];
        if (!name || !Object.prototype.hasOwnProperty.call(vars, name)) {
          return <Fragment key={i}>{part}</Fragment>;
        }
        const value = String(vars[name]);
        const title = titles?.[name];
        return !mono || mono.includes(name) ? (
          <span key={i} className="mono tabular-nums" title={title}>
            <bdi>{value}</bdi>
          </span>
        ) : (
          <bdi key={i} title={title}>
            {value}
          </bdi>
        );
      });
    },
    [locale],
  );
}
