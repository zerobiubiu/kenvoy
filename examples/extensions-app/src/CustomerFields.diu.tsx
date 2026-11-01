export default (
  <>
    <text name="name" label="客户姓名" required maxLength={100} />
    <text name="phone" label="联系电话" />
    <number name="age" label="年龄" integer min={0} />
  </>
);
