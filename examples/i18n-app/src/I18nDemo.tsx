import { useState } from "react";
import { useI18n, useLocale } from "@kenvoy/client";

export function I18nDemo() {
    const locale = useLocale();
    const { t, formatNumber, formatDate, formatDateTime } = useI18n();
    const [count, setCount] = useState(1);
    return (
        <section className="kv-stack" aria-label={t("demo.title")}>
            <h2>{t("demo.title")}</h2>
            <p>{t("demo.greeting", { name: "Kenvoy" })}</p>
            <p>{t("demo.locale", { locale })}</p>
            <div className="kv-row">
                <strong className="i18n-count">
                    {t("demo.people", { count })}
                </strong>
                <button
                    type="button"
                    className="kv-button"
                    onClick={() => setCount((value) => value + 1)}
                >
                    {t("demo.add")}
                </button>
            </div>
            <dl className="kv-stack">
                <div>
                    <dt>{t("demo.number")}</dt>
                    <dd>{formatNumber(12345.6789)}</dd>
                </div>
                <div>
                    <dt>{t("demo.date")}</dt>
                    <dd>{formatDate("2030-05-20", { dateStyle: "long" })}</dd>
                </div>
                <div>
                    <dt>{t("demo.datetime")}</dt>
                    <dd>
                        {formatDateTime("2030-05-20T10:30:00+08:00", {
                            dateStyle: "long",
                            timeStyle: "short",
                            hour12: false,
                        })}
                    </dd>
                </div>
            </dl>
            <p className="kv-hint">{t("demo.note")}</p>
        </section>
    );
}
