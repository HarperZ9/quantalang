; QuantaLang LLVM IR Output
; Module: main

source_filename = "main"
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"

%quanta_vec2 = type { double, double }
%quanta_vec3 = type { double, double, double }
%quanta_vec4 = type { double, double, double, double }


; Runtime types
%QuantaString = type { ptr, i64, i64 }
%QuantaVec = type { ptr, i64, i64, i64 }
%QuantaHandler = type { [64 x i8], i32, ptr, ptr }

@.str.0 = private unnamed_addr constant [15 x i8] c"Hello, World!\0A\00", align 1

; LLVM Intrinsics
declare void @llvm.memcpy.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg) nounwind
declare void @llvm.memmove.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg) nounwind
declare void @llvm.memset.p0.i64(ptr nocapture writeonly, i8, i64, i1 immarg) nounwind
declare float @llvm.sqrt.f32(float) nounwind readnone
declare float @llvm.sin.f32(float) nounwind readnone
declare float @llvm.cos.f32(float) nounwind readnone
declare float @llvm.pow.f32(float, float) nounwind readnone
declare float @llvm.exp.f32(float) nounwind readnone
declare float @llvm.log.f32(float) nounwind readnone
declare float @llvm.fabs.f32(float) nounwind readnone
declare float @llvm.floor.f32(float) nounwind readnone
declare float @llvm.ceil.f32(float) nounwind readnone
declare float @llvm.round.f32(float) nounwind readnone
declare float @llvm.fma.f32(float, float, float) nounwind readnone
declare float @llvm.minnum.f32(float, float) nounwind readnone
declare float @llvm.maxnum.f32(float, float) nounwind readnone
declare float @llvm.copysign.f32(float, float) nounwind readnone
declare double @llvm.sqrt.f64(double) nounwind readnone
declare double @llvm.sin.f64(double) nounwind readnone
declare double @llvm.cos.f64(double) nounwind readnone
declare double @llvm.pow.f64(double, double) nounwind readnone
declare double @llvm.exp.f64(double) nounwind readnone
declare double @llvm.log.f64(double) nounwind readnone
declare double @llvm.fabs.f64(double) nounwind readnone
declare double @llvm.floor.f64(double) nounwind readnone
declare double @llvm.ceil.f64(double) nounwind readnone
declare double @llvm.round.f64(double) nounwind readnone
declare double @llvm.fma.f64(double, double, double) nounwind readnone
declare double @llvm.minnum.f64(double, double) nounwind readnone
declare double @llvm.maxnum.f64(double, double) nounwind readnone
declare double @llvm.copysign.f64(double, double) nounwind readnone
declare i8 @llvm.ctpop.i8(i8) nounwind readnone
declare i16 @llvm.ctpop.i16(i16) nounwind readnone
declare i32 @llvm.ctpop.i32(i32) nounwind readnone
declare i64 @llvm.ctpop.i64(i64) nounwind readnone
declare i32 @llvm.ctlz.i32(i32, i1 immarg) nounwind readnone
declare i64 @llvm.ctlz.i64(i64, i1 immarg) nounwind readnone
declare i32 @llvm.cttz.i32(i32, i1 immarg) nounwind readnone
declare i64 @llvm.cttz.i64(i64, i1 immarg) nounwind readnone
declare i32 @llvm.bswap.i32(i32) nounwind readnone
declare i64 @llvm.bswap.i64(i64) nounwind readnone
declare i32 @llvm.bitreverse.i32(i32) nounwind readnone
declare i64 @llvm.bitreverse.i64(i64) nounwind readnone
declare {i32, i1} @llvm.sadd.with.overflow.i32(i32, i32) nounwind readnone
declare {i64, i1} @llvm.sadd.with.overflow.i64(i64, i64) nounwind readnone
declare {i32, i1} @llvm.uadd.with.overflow.i32(i32, i32) nounwind readnone
declare {i64, i1} @llvm.uadd.with.overflow.i64(i64, i64) nounwind readnone
declare {i32, i1} @llvm.ssub.with.overflow.i32(i32, i32) nounwind readnone
declare {i64, i1} @llvm.ssub.with.overflow.i64(i64, i64) nounwind readnone
declare {i32, i1} @llvm.smul.with.overflow.i32(i32, i32) nounwind readnone
declare {i64, i1} @llvm.smul.with.overflow.i64(i64, i64) nounwind readnone
declare i32 @llvm.sadd.sat.i32(i32, i32) nounwind readnone
declare i64 @llvm.sadd.sat.i64(i64, i64) nounwind readnone
declare i32 @llvm.uadd.sat.i32(i32, i32) nounwind readnone
declare i64 @llvm.uadd.sat.i64(i64, i64) nounwind readnone
declare i32 @llvm.ssub.sat.i32(i32, i32) nounwind readnone
declare i64 @llvm.ssub.sat.i64(i64, i64) nounwind readnone
declare void @llvm.lifetime.start.p0(i64 immarg, ptr nocapture) nounwind
declare void @llvm.lifetime.end.p0(i64 immarg, ptr nocapture) nounwind
declare void @llvm.dbg.declare(metadata, metadata, metadata) nounwind readnone
declare void @llvm.dbg.value(metadata, metadata, metadata) nounwind readnone
declare void @llvm.trap() cold noreturn nounwind
declare void @llvm.debugtrap() nounwind
declare ptr @llvm.stacksave.p0() nounwind
declare void @llvm.stackrestore.p0(ptr) nounwind
declare void @llvm.assume(i1) nounwind
declare i1 @llvm.expect.i1(i1, i1) nounwind readnone
declare i64 @llvm.expect.i64(i64, i64) nounwind readnone
declare void @llvm.prefetch.p0(ptr, i32 immarg, i32 immarg, i32 immarg) nounwind

; Implicit external declarations
declare i32 @printf(ptr, ...) nounwind

define external i32 @main()  {
entry:
  %local0 = alloca i32, align 4
  %local1 = alloca ptr, align 8
  br label %bb0

bb0:
  store ptr @.str.0, ptr %local1, align 8
  %0 = load ptr, ptr %local1, align 8
  %1 = call i32 @printf(ptr %0)
  br label %bb1

bb1:
  ret i32 0
}

