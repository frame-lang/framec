@@system LifecycleArgs {
    interface:
        load(n: int, label: char*)
        total(): int
        tag(): char*

    machine:
        $Idle {
            load(n: int, label: char*) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: int, name: char*) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): int {
                @@:(self.sum)
                return
            }
            tag(): char* {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: int = 0
        label: char* = ""
}
