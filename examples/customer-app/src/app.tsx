export default (
  <app name="客户管理系统">
    <resource name="customers">
      <text name="name" label="客户姓名" required maxLength={100} />
      <text name="phone" label="联系电话" />
      <number name="age" label="年龄" integer min={0} />
    </resource>
    <page path="/customers" title="客户登记">
      <form resource="customers" mode="create" />
    </page>
  </app>
);
