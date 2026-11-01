import CustomerFields from './CustomerFields.diu';
import { CustomerCard } from './components';
import { calculateDiscount } from './discount.server';

const step = 2;

export default (
  <app name="客户管理扩展示例">
    <resource name="customers">
      <CustomerFields />
    </resource>

    <page path="/customers" title="客户登记">
      <CustomerCard title="自定义客户卡片" onIncrement={(count: number) => count + step} />
      <form resource="customers" mode="create" />
    </page>

    <action
      name="calculateDiscount"
      handler={calculateDiscount}
      runtime="server"
      permission="anonymous"
    />
  </app>
);
