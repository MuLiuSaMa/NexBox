# ==== kotlinx.serialization ====
# 序列化类的字段名就是 JSON 里的键名，参与与 PC 端的互通协议，不能被混淆。
-keepattributes *Annotation*, InnerClasses, Signature, RuntimeVisible*Annotations, AnnotationDefault
-keepclassmembers class com.nexbox.app.** {
    *** Companion;
}
-keepclasseswithmembers class com.nexbox.app.** {
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class com.nexbox.app.**$$serializer { *; }
-if @kotlinx.serialization.Serializable class com.nexbox.app.**
-keep class com.nexbox.app.<1> {
    public static ** INSTANCE;
    kotlinx.serialization.KSerializer serializer(...);
}
