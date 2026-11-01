import { CustomSelect, CustomForm, ResourceSummary } from './components';

export default <app name="联动选项与组件重载">
  <override name="select" component={CustomSelect} />
  <override name="form" component={CustomForm} />
  <resource name="cities">
    <text name="code" label="城市代码" required unique />
    <text name="name" label="城市名称" required />
    <text name="province" label="省份代码" required />
    <text name="private_note" label="内部备注" />
  </resource>
  <resource name="entries">
    <text name="name" label="客户姓名" required />
    <select name="province" label="省份" required>
      <option value="zj" label="浙江" />
      <option value="js" label="江苏" />
    </select>
    <select name="service" label="服务区" required>
      <option value="east" label="浙江服务区" when={{ province: 'zj' }} />
      <option value="north" label="江苏服务区" when={{ province: 'js' }} />
    </select>
    <select name="city" label="城市" required optionsSource={{
      resource: 'cities', value: 'code', label: 'name',
      filters: { province: 'province' }, permission: 'anonymous',
    }} />
  </resource>
  <page path="/entries" title="客户城市登记">
    <ResourceSummary />
    <form resource="entries" />
  </page>
</app>;
