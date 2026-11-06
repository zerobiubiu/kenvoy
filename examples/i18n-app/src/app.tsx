import { I18nDemo } from "./I18nDemo";

export default function App() {
    return (
        <app name="国际化报名示例">
            <resource name="registrations">
                <text name="name" label="姓名" required maxLength={80} />
                <number
                    name="guests"
                    label="同行人数"
                    integer
                    min={1}
                    max={10}
                    default={1}
                />
                <select
                    name="session"
                    label="参加场次"
                    required
                    default="morning"
                >
                    <option value="morning" label="上午场" />
                    <option value="afternoon" label="下午场" />
                </select>
                <boolean name="newsletter" label="接收通知" default={false} />
                <date name="arrival" label="到访日期" />
            </resource>
            <page path="/register" title="活动报名">
                <I18nDemo />
                <group label="填写报名信息">
                    <form resource="registrations" mode="create" />
                </group>
            </page>
            <page path="/registrations" title="报名查询" permission="admin">
                <table resource="registrations" />
            </page>
            <navigation label="报名导航">
                <item path="/register" label="提交报名" />
                <item path="/registrations" label="查看报名" />
            </navigation>
        </app>
    );
}
