import { UploadGallery } from './gallery';
import { EXAMPLE_IMAGE_URL } from './image-source';

export default <app name="文件上传与图片示例">
  <resource name="materials">
    <text name="title" label="资料名称" required maxLength={100} />
    <textarea name="description" label="资料说明" maxLength={500} />
    <file name="photos" label="图片" required maxFiles={4} maxSize={5242880}
      accept={["image/png", "image/jpeg", "image/gif", "image/webp"]} />
    <file name="attachments" label="补充附件" maxFiles={3} maxSize={2097152}
      accept={["application/pdf", "text/plain"]} />
  </resource>
  <page path="/uploads" title="上传资料">
    <form resource="materials" />
  </page>
  <page path="/images" title="图片 URL 展示">
    <group label="声明式 image 组件">
      <image src={EXAMPLE_IMAGE_URL} alt="Rust 官方徽标，外部图片 URL 示例"
        width={256} height={256} fit="contain" />
    </group>
  </page>
  <page path="/gallery" title="已上传资料" permission="admin">
    <UploadGallery />
  </page>
</app>;
