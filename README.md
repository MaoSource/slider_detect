# SliderDetect

`SliderDetect.dll` 用于识别滑块验证码缺口坐标。输入带缺口阴影的背景图路径，可选输入滑块图片路径，返回坐标字符串。

## 导出函数

```rust
FindSliderPosition(bg_path, slider_path) -> *mut c_char
FreeResult(ptr)
```

`FindSliderPosition` 使用 C ABI，供 32 位易语言调用。返回值由 Rust 通过 `CString::into_raw()` 分配，调用方使用完字符串后必须调用 `FreeResult` 释放。

## 参数

- `bg_path`：带缺口阴影的背景图路径，UTF-8 字符串，不能为空。
- `slider_path`：滑块图路径，UTF-8 字符串。可以传空指针或空字符串。

## 返回值

- 成功：`"x,y"`，例如 `"126,70"`。
- 失败：`"-1,-1"`。

## 识别逻辑

有滑块图时优先使用 OpenCV 模板匹配：背景图和滑块图转灰度后做 Canny 边缘提取，再用 `matchTemplate` 匹配。如果滑块图有 alpha 通道，会优先用 alpha 生成 mask；否则使用滑块边缘作为 mask。

没有滑块图时使用轮廓检测兜底：灰度化、GaussianBlur、Canny、闭运算、`findContours`，再按尺寸、面积、宽高比例等规则选择最像缺口的候选。轮廓检测准确率受图片背景、缺口阴影强度和干扰纹理影响。

## 易语言调用说明

请使用 32 位易语言加载 32 位 DLL。`FindSliderPosition` 返回的是字符串指针，读取字符串后必须调用 `FreeResult`。

示例声明思路：

```text
FindSliderPosition(bg_path: 文本型, slider_path: 文本型) -> 整数型/长整数型指针
FreeResult(ptr: 整数型/长整数型指针)
```

调用流程：

1. 传入 `bg_path`。
2. 有滑块图时传入 `slider_path`，没有滑块图时传空字符串。
3. 将返回指针按 UTF-8/ANSI 字符串读取为 `"x,y"`。
4. 调用 `FreeResult(ptr)` 释放返回字符串。

## GitHub Actions 打包

推送到 GitHub 后，工作流会在 `windows-latest` 上执行：

```text
cargo build --release --target i686-pc-windows-msvc
```

产物会上传为 `SliderDetect-windows-x86`，至少包含：

- `slider_detect.dll`
- `SliderDetect.dll`，内容与 `slider_detect.dll` 相同，方便按建议名称分发
- `README.md`
- vcpkg `x86-windows/bin` 下的运行时 DLL，包括 OpenCV 及其依赖 DLL

最终 DLL 路径：

```text
target/i686-pc-windows-msvc/release/slider_detect.dll
```

如果分发给易语言程序，请将 `SliderDetect.dll` 和 artifact 中的运行时 DLL 放在同一目录，或放到系统 `PATH` 能找到的位置。
