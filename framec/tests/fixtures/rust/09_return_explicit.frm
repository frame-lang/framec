@@system ReturnExplicit {
    interface:
        decide(score: i32): String

    machine:
        $Judging {
            decide(score: i32): String {
                if score >= 60 {
                    @@:return(String::from("pass"))
                }
                @@:return(String::from("fail"))
            }
        }
}
