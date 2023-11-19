
问答作业：

1. 因为采用8bit 无符号整数存储stride的话，那么由于溢出 250 + 10 = 4, 所以执行的p1
2. BigStride / 2 表示的是最大能加上的 pass。每个的进程的初始 stride = 0，所以当最低优先级的进程加上最大pass后，其他进程只要stride超过前面那个最低优先级的进程，就会暂停运行了。
   之所以优先级 $\le2$ 是为了方便进行溢出检测
3. ```rust
   impl PartialOrd for Stride {
       fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
   	if i32::abs((self.0 as i32) - (other.0 as i32)) > BigStride / 2 {
   	    if self.0 < other.0 {
   		Some(Ordering::Greater)
   	    } else {
   		Some(Ordering::Less)
   	    }
   	} else {
   	    Some(self.cmp(other))
   	}
       }
   }
   
   impl PartialEq for Stride {
       fn eq(&self, other: &Self) -> bool {
           self.0 == other.0
       }
   }
   ```



#### 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

   > nothing

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

   > nothing

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
