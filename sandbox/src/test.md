## Feature Branches

```bob
main
  |
  * v0.1.0
  |---> feature-branch --> "impl-branch-1"
  |         |   \               |
  *         |    \              * PR approved
  |         |     \            /
  *<-...    *<----------------+ PR merged
  |         |       \
  |         |        +-> "impl-branch-2"
  |         |                 | 
  *<--...   |                 * 
  |         |                 |
  * v0.1.1  |                 * PR approved
  |         |                /
  *<-...    *<--------------+ PR merged 
  |         |
  * v0.1.2  * PR approved
  |        /
  *<------+ PR merged
  |
  * v0.2.0
```

This workflow model is effective only in the number of feature branches is low and can be tamed.
