@@system ReturnExplicit {
    interface:
        decide(score: int): char*

    machine:
        $Judging {
            decide(score: int): char* {
                if (score >= 60) {
                    @@:return("pass")
                }
                @@:return("fail")
            }
        }
}
