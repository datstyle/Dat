// lib.rs - Course Management Smart Contract (Soroban / Stellar)

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    Address, Env, String, Vec, Map,
    symbol_short, log,
};

// =====================
//  DATA STRUCTURES
// =====================

#[contracttype]
#[derive(Clone, Debug)]
pub struct Course {
    pub id: u64,
    pub title: String,
    pub instructor: Address,
    pub price: i128,        // in stroops (1 XLM = 10_000_000 stroops)
    pub max_students: u32,
    pub enrolled_count: u32,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Enrollment {
    pub student: Address,
    pub course_id: u64,
    pub enrolled_at: u64,   // ledger timestamp
    pub is_completed: bool,
}

// Storage keys
#[contracttype]
pub enum DataKey {
    Admin,
    CourseCount,
    Course(u64),
    Enrollment(Address, u64),   // (student, course_id)
    StudentCourses(Address),    // list of course IDs for a student
    CourseStudents(u64),        // list of students for a course
}

// =====================
//  CONTRACT
// =====================

#[contract]
pub struct CourseManagement;

#[contractimpl]
impl CourseManagement {

    // ----- INIT -----

    /// Initialize contract, set admin
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::CourseCount, &0u64);
        log!(&env, "Contract initialized. Admin: {}", admin);
    }

    // ----- ADMIN -----

    /// Get current admin
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    /// Transfer admin role
    pub fn transfer_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    // ----- COURSE MANAGEMENT -----

    /// Create a new course (only instructor, no admin restriction)
    pub fn create_course(
        env: Env,
        instructor: Address,
        title: String,
        price: i128,
        max_students: u32,
    ) -> u64 {
        instructor.require_auth();

        if price < 0 {
            panic!("Price must be non-negative");
        }
        if max_students == 0 {
            panic!("Max students must be > 0");
        }

        let course_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CourseCount)
            .unwrap_or(0);

        let new_id = course_count + 1;

        let course = Course {
            id: new_id,
            title,
            instructor,
            price,
            max_students,
            enrolled_count: 0,
            is_active: true,
        };

        env.storage().persistent().set(&DataKey::Course(new_id), &course);
        env.storage().instance().set(&DataKey::CourseCount, &new_id);

        // Init empty student list for this course
        let empty: Vec<Address> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::CourseStudents(new_id), &empty);

        log!(&env, "Course created. ID: {}", new_id);
        new_id
    }

    /// Update course info (only instructor of that course)
    pub fn update_course(
        env: Env,
        course_id: u64,
        title: Option<String>,
        price: Option<i128>,
        max_students: Option<u32>,
    ) {
        let mut course: Course = Self::get_course_or_panic(&env, course_id);
        course.instructor.require_auth();

        if let Some(t) = title {
            course.title = t;
        }
        if let Some(p) = price {
            if p < 0 { panic!("Price must be non-negative"); }
            course.price = p;
        }
        if let Some(m) = max_students {
            if m < course.enrolled_count {
                panic!("Cannot set max_students below current enrollment");
            }
            course.max_students = m;
        }

        env.storage().persistent().set(&DataKey::Course(course_id), &course);
    }

    /// Deactivate a course (only instructor or admin)
    pub fn deactivate_course(env: Env, caller: Address, course_id: u64) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();

        let mut course: Course = Self::get_course_or_panic(&env, course_id);

        if caller != admin && caller != course.instructor {
            panic!("Only admin or instructor can deactivate");
        }

        course.is_active = false;
        env.storage().persistent().set(&DataKey::Course(course_id), &course);
    }

    /// Reactivate a course (only instructor or admin)
    pub fn activate_course(env: Env, caller: Address, course_id: u64) {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();

        let mut course: Course = Self::get_course_or_panic(&env, course_id);

        if caller != admin && caller != course.instructor {
            panic!("Only admin or instructor can activate");
        }

        course.is_active = true;
        env.storage().persistent().set(&DataKey::Course(course_id), &course);
    }

    // ----- ENROLLMENT -----

    /// Enroll a student in a course
    /// Note: payment logic should be handled via token contract separately
    pub fn enroll(env: Env, student: Address, course_id: u64) {
        student.require_auth();

        let mut course: Course = Self::get_course_or_panic(&env, course_id);

        if !course.is_active {
            panic!("Course is not active");
        }
        if course.enrolled_count >= course.max_students {
            panic!("Course is full");
        }

        // Check not already enrolled
        if env
            .storage()
            .persistent()
            .has(&DataKey::Enrollment(student.clone(), course_id))
        {
            panic!("Already enrolled in this course");
        }

        // Create enrollment record
        let enrollment = Enrollment {
            student: student.clone(),
            course_id,
            enrolled_at: env.ledger().timestamp(),
            is_completed: false,
        };

        env.storage().persistent().set(
            &DataKey::Enrollment(student.clone(), course_id),
            &enrollment,
        );

        // Update course enrolled count
        course.enrolled_count += 1;
        env.storage().persistent().set(&DataKey::Course(course_id), &course);

        // Add course to student's list
        let mut student_courses: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::StudentCourses(student.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        student_courses.push_back(course_id);
        env.storage()
            .persistent()
            .set(&DataKey::StudentCourses(student.clone()), &student_courses);

        // Add student to course's list
        let mut course_students: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CourseStudents(course_id))
            .unwrap_or_else(|| Vec::new(&env));
        course_students.push_back(student.clone());
        env.storage()
            .persistent()
            .set(&DataKey::CourseStudents(course_id), &course_students);

        log!(&env, "Student enrolled. Course: {}", course_id);
    }

    /// Mark a student's course as completed (only instructor of the course)
    pub fn complete_course(env: Env, instructor: Address, student: Address, course_id: u64) {
        instructor.require_auth();

        let course: Course = Self::get_course_or_panic(&env, course_id);
        if course.instructor != instructor {
            panic!("Only the course instructor can mark completion");
        }

        let key = DataKey::Enrollment(student.clone(), course_id);
        let mut enrollment: Enrollment = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("Enrollment not found"));

        if enrollment.is_completed {
            panic!("Already marked as completed");
        }

        enrollment.is_completed = true;
        env.storage().persistent().set(&key, &enrollment);

        log!(&env, "Course {} completed by student", course_id);
    }

    /// Cancel enrollment (student withdraws)
    pub fn cancel_enrollment(env: Env, student: Address, course_id: u64) {
        student.require_auth();

        let key = DataKey::Enrollment(student.clone(), course_id);
        if !env.storage().persistent().has(&key) {
            panic!("Enrollment not found");
        }

        let enrollment: Enrollment = env.storage().persistent().get(&key).unwrap();
        if enrollment.is_completed {
            panic!("Cannot cancel a completed enrollment");
        }

        env.storage().persistent().remove(&key);

        // Decrement enrolled_count
        let mut course: Course = Self::get_course_or_panic(&env, course_id);
        if course.enrolled_count > 0 {
            course.enrolled_count -= 1;
        }
        env.storage().persistent().set(&DataKey::Course(course_id), &course);

        log!(&env, "Enrollment cancelled. Course: {}", course_id);
    }

    // ----- QUERIES -----

    /// Get a course by ID
    pub fn get_course(env: Env, course_id: u64) -> Course {
        Self::get_course_or_panic(&env, course_id)
    }

    /// Get total number of courses
    pub fn get_course_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::CourseCount).unwrap_or(0)
    }

    /// Get enrollment details
    pub fn get_enrollment(env: Env, student: Address, course_id: u64) -> Enrollment {
        env.storage()
            .persistent()
            .get(&DataKey::Enrollment(student, course_id))
            .unwrap_or_else(|| panic!("Enrollment not found"))
    }

    /// Check if a student is enrolled in a course
    pub fn is_enrolled(env: Env, student: Address, course_id: u64) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Enrollment(student, course_id))
    }

    /// Get all course IDs a student is enrolled in
    pub fn get_student_courses(env: Env, student: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::StudentCourses(student))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get all students enrolled in a course
    pub fn get_course_students(env: Env, course_id: u64) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::CourseStudents(course_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    // =====================
    //  PRIVATE HELPERS
    // =====================

    fn get_course_or_panic(env: &Env, course_id: u64) -> Course {
        env.storage()
            .persistent()
            .get(&DataKey::Course(course_id))
            .unwrap_or_else(|| panic!("Course not found"))
    }
}

// =====================
//  TESTS
// =====================

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    #[test]
    fn test_full_flow() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CourseManagement);
        let client = CourseManagementClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let instructor = Address::generate(&env);
        let student = Address::generate(&env);

        // Init
        client.initialize(&admin);
        assert_eq!(client.get_admin(), admin);

        // Create course
        let course_id = client.create_course(
            &instructor,
            &String::from_str(&env, "Rust for Blockchain"),
            &1_000_000i128,
            &50u32,
        );
        assert_eq!(course_id, 1);

        // Get course
        let course = client.get_course(&course_id);
        assert_eq!(course.enrolled_count, 0);
        assert!(course.is_active);

        // Enroll
        client.enroll(&student, &course_id);
        assert!(client.is_enrolled(&student, &course_id));

        let updated = client.get_course(&course_id);
        assert_eq!(updated.enrolled_count, 1);

        // Complete
        client.complete_course(&instructor, &student, &course_id);
        let enrollment = client.get_enrollment(&student, &course_id);
        assert!(enrollment.is_completed);

        // Deactivate
        client.deactivate_course(&instructor, &course_id);
        let deactivated = client.get_course(&course_id);
        assert!(!deactivated.is_active);
    }

    #[test]
    #[should_panic(expected = "Course is full")]
    fn test_enroll_full_course() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CourseManagement);
        let client = CourseManagementClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let instructor = Address::generate(&env);

        client.initialize(&admin);
        let course_id = client.create_course(
            &instructor,
            &String::from_str(&env, "Limited Course"),
            &0i128,
            &1u32,  // max 1 student
        );

        let s1 = Address::generate(&env);
        let s2 = Address::generate(&env);
        client.enroll(&s1, &course_id);
        client.enroll(&s2, &course_id); // should panic
    }
}