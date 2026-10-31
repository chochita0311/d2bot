# Third-Party Algorithm Notices

The Windows package includes a Cargo normal-dependency notice index under `licenses/cargo/index.json`, retaining each exact package version, declared license expression and upstream notice files, including embedded font notices. For registry archives lacking workspace-level texts, [pinned upstream sources](licenses/cargo-upstream/sources.json) and their SHA256 values supply the retained originals. The egui family uses the official0.37.0 release-tag licenses because the archive VCS hashes did not resolve publicly; AccessKit/profiling/clipboard notices use their archive-recorded commits. These are notice provenance records, not new executable dependencies.

The retained BGR8 rectangle/circle/Hershey Simplex annotation rasterizer and lookup data are adapted from OpenCV `modules/imgproc/src/{drawing,hershey_fonts}.cpp` at the same pinned commit `b4c5ec4042f097e2a5b386b9d413ec7333d0a184`. The source-specific [drawing/font BSD notices](licenses/opencv-drawing-bsd.txt) and OpenCV Apache 2.0 text accompany the adaptation. SHA256 values are `feb12ebea7b08419289a46d043afeee665b77217d1faed89d86ad30d2d8af0e0` and `5fa0d65264f72e5b166d07973aabd49f27cc54d0074062a8efc312bf9ab95432`; the generator records and verifies them. The Rust executable does not load the reference C++ files or OpenCV.

The MPEG-4 VLC data in `src/common/recording/mpeg4/tables.rs` is adapted from the publicly available Android/PacketVideo MPEG-4 reference at commit `1e6a675fccdc37afcfb337f8fa97d1ddab90adf0`, `media/codecs/m4v_h263/enc/src/vlc_enc_tab.h`. Its [PacketVideo Apache 2.0 and ISO/ACTS-MoMuSys notices](licenses/mpeg4-reference-notice.txt) are retained, together with the existing [Apache 2.0 license text](licenses/opencv-apache-2.0.txt). The generator records the exact source hash. Header/transform/prediction reference reading does not introduce a C codec runtime; the Rust bitstream implementation is checked independently and does not claim formal profile-level certification for arbitrary desktop resolutions.

The Rust partial-affine/RANSAC implementation is adapted from OpenCV's `modules/calib3d/src/ptsetreg.cpp`, pinned to [b4c5ec4042f097e2a5b386b9d413ec7333d0a184](https://github.com/opencv/opencv/blob/b4c5ec4042f097e2a5b386b9d413ec7333d0a184/modules/calib3d/src/ptsetreg.cpp). This is source attribution; the Rust runtime does not link OpenCV. Cargo dependencies retain their own upstream licenses.

The Rust ORB600 extraction, BRIEF sampling pattern, FAST score, float-angle and image pyramid arithmetic are adapted from the same OpenCV commit's `modules/features2d/src/{orb,fast,fast_score,keypoint}.cpp`, `modules/core/src/mathfuncs_core.simd.hpp`, `modules/imgproc/src/resize.cpp`, `filter.dispatch.cpp`, `smooth.dispatch.cpp` and their SIMD helpers. The original [ORB BSD notice](licenses/opencv-orb-bsd.txt), [FAST BSD notice](licenses/opencv-fast-bsd.txt) and [OpenCV Apache 2.0 license](licenses/opencv-apache-2.0.txt) are retained for source and binary redistribution.

Keypoint selection reproduces the observable `nth_element` / bidirectional `partition` ordering of Microsoft STL shipped with MSVC 14.44.35207. Copyright (c) Microsoft Corporation. SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception. The adaptation in `src/common/scene_motion/selection.rs` retains the [Microsoft STL Apache 2.0 license with LLVM exception](licenses/msvc-stl-apache-2.0-with-llvm-exception.txt), retrieved from the official [vs-2022-17.14 source](https://github.com/microsoft/STL/tree/vs-2022-17.14). These notices and license files must accompany the final binary distribution.

Additional source-specific BSD notices are retained for [resize](licenses/opencv-resize-bsd.txt), [separable filters](licenses/opencv-filter-bsd.txt) and [keypoint selection](licenses/opencv-keypoint-bsd.txt). Source hashes, installed baseline versions and fixed comparison limits are recorded in [motion-baseline.json](tests/fixtures/rust-port/motion-baseline.json).

Town/title, HUD and body recognition also reproduce the pinned `smooth.dispatch.cpp` symmetric error-diffusion coefficients and Q8 separable arithmetic for whole-image Gaussian kernels (3/sigma0.8, 15/sigma0 and 31/sigma5), and explicit-fx label width resizing and half-size coarse matching from `resize.cpp`. Navigation grid sampling reproduces that file's integer-area and float-working fractional-area arithmetic and coefficient order for both BGR pixels and float neutral coverage. The source-specific [Gaussian BSD notice](licenses/opencv-smooth-bsd.txt) and [resize BSD notice](licenses/opencv-resize-bsd.txt) apply to these adaptations as well.

Reviewed UI color distances reproduce the float32 pairwise reduction ordering from NumPy 1.21.6's [`numpy/core/src/umath/loops_utils.h.src`](https://github.com/numpy/numpy/blob/v1.21.6/numpy/core/src/umath/loops_utils.h.src) and scalar mean conversion from [`numpy/core/_methods.py`](https://github.com/numpy/numpy/blob/v1.21.6/numpy/core/_methods.py). The [NumPy BSD notice](licenses/numpy-bsd.txt) accompanies this adaptation. The Rust runtime does not load NumPy.

The uint8 BGR-to-Lab path reproduces Q12/Q15 matrix and lookup-table arithmetic from the same pinned [`color_lab.cpp`](https://github.com/opencv/opencv/blob/b4c5ec4042f097e2a5b386b9d413ec7333d0a184/modules/imgproc/src/color_lab.cpp), and the positive normal-input cube-root table approximation from [`softfloat.cpp`](https://github.com/opencv/opencv/blob/b4c5ec4042f097e2a5b386b9d413ec7333d0a184/modules/core/src/softfloat.cpp). The OpenCV Apache 2.0 license and [SoftFloat/FDLIBM source notices](licenses/opencv-softfloat-notices.txt) accompany these adaptations. The original RGB-to-Lab function credits the MATLAB script RGB2Lab.m translated by Mark Ruzon from C code by Yossi Rubner, 23 September 1997; the fast cube-root implementation credits Ken Turkowski. Retrieved source SHA256 values are `ab867ecd482d3f26a2fb2111b1224903e40a809b00f17f30276b6e00d934b8b6` and `439d023e20307bcefad2309e486949ddd605a42acf7e4ba6118858812564c9f8`, respectively. No OpenCV runtime is used.

## OpenCV Partial-Affine And RANSAC Source Notice

```text
License Agreement
For Open Source Computer Vision Library

Copyright (C) 2000-2008, Intel Corporation, all rights reserved.
Copyright (C) 2009, Willow Garage Inc., all rights reserved.
Copyright (C) 2013, OpenCV Foundation, all rights reserved.
Third party copyrights are property of their respective owners.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

  * Redistribution's of source code must retain the above copyright notice,
    this list of conditions and the following disclaimer.

  * Redistribution's in binary form must reproduce the above copyright notice,
    this list of conditions and the following disclaimer in the documentation
    and/or other materials provided with the distribution.

  * The name of the copyright holders may not be used to endorse or promote products
    derived from this software without specific prior written permission.

This software is provided by the copyright holders and contributors "as is" and
any express or implied warranties, including, but not limited to, the implied
warranties of merchantability and fitness for a particular purpose are disclaimed.
In no event shall the Intel Corporation or contributors be liable for any direct,
indirect, incidental, special, exemplary, or consequential damages
(including, but not limited to, procurement of substitute goods or services;
loss of use, data, or profits; or business interruption) however caused
and on any theory of liability, whether in contract, strict liability,
or tort (including negligence or otherwise) arising in any way out of
the use of this software, even if advised of the possibility of such damage.
```
