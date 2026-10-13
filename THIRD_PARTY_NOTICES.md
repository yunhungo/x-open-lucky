# 第三方组件

`frontend/src/components/ui/` 使用官方 shadcn/ui registry 的组件，并调整了工具函数引用。shadcn/ui 使用 MIT 许可证。

Copyright (c) 2023 shadcn

以下组件适用下列 MIT 许可条款：shadcn/ui、lego、h3 / h3-quinn 和 Quinn（选择其 MIT 许可）。

Copyright (c) 2020 h3 authors
Copyright (c) 2018 The quinn Developers

Copyright (c) 2017-2024 Ludovic Fernandez
Copyright (c) 2015-2017 Sebastian Erhart

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

生产镜像包含 go-acme/lego 的公开发布二进制，用于 ACME 协议及 DNS-01 挑战；lego 使用 MIT 许可证，源码与声明见 <https://github.com/go-acme/lego>。依赖许可证由 Cargo/npm 依赖包保留；项目未复制 Lucky 的实现代码。

HTTP/3 使用 [h3 / h3-quinn](https://github.com/hyperium/h3) 与 [Quinn](https://github.com/quinn-rs/quinn)。动态 Header 模板使用 [MiniJinja](https://github.com/mitsuhiko/minijinja)，Copyright 2022 Armin Ronacher，使用 Apache-2.0 许可证，完整条款见 [LICENSE-APACHE-2.0](LICENSE-APACHE-2.0)。上述库通过 Cargo 引入，未修改其源码。
